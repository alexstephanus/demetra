use core::{
    future::Future,
    iter::Iterator,
    pin::pin,
    task::{Context, Poll, Waker},
};

use embassy_time::{Duration, Instant, Timer};

use esp_hal::{
    gpio::{Input, Output},
    ledc::{
        channel::{Channel as LedcChannel, ChannelIFace, Error as LedcChannelError},
        LowSpeed,
    },
    rmt::{Channel, Error as RmtError, PulseCode, Rx},
    Async,
};

use log::debug;

type MuxPinError = port_expander::PinError<
    embassy_embedded_hal::shared_bus::I2cDeviceError<esp_hal::i2c::master::Error>,
>;

#[derive(Debug)]
#[allow(dead_code)]
enum SensorReadError {
    IsolatedPowerProblem,
    ClockStartFailure(LedcChannelError),
    MuxSelect(MuxPinError),
    Rmt(RmtError),
    InsufficientData,
}

use lib::{
    peripherals::{cic_filter_order_3, OversampleRatio, SensorError, SensorReadRaw},
    ui_types::SensorType,
};

use super::type_aliases::McpOutputPin;

const OSR: OversampleRatio = OversampleRatio::_1024;
const OSR_DIVISOR: u32 = OSR.get_cnc_filter_cap();
const SINC_FILTER_ORDER: usize = 3;
const RMT_BUFFER_LEN: usize = OSR as usize * (SINC_FILTER_ORDER + 1) / 2 + 1;

const VOLTAGE_REFERENCE_MV: f32 = 1024.0;
const ADC_RANGE: f32 = 2.5;

pub struct RmtReadings<'a> {
    items: &'a [PulseCode; RMT_BUFFER_LEN],
}

impl<'a> RmtReadings<'a> {
    fn new(items: &'a [PulseCode; RMT_BUFFER_LEN]) -> Self {
        Self { items }
    }
}

impl<'a> IntoIterator for &'a RmtReadings<'a> {
    type Item = bool;
    type IntoIter = RmtBitIterator<'a>;

    fn into_iter(self) -> RmtBitIterator<'a> {
        RmtBitIterator::new(&self.items)
    }
}

pub struct RmtBitIterator<'a> {
    items: &'a [PulseCode],
    pulse_code_value: bool,
    pulse_code_quantity: u16,
    bit_idx: usize,
    first_pulse_in_code: bool,
}

impl<'a> RmtBitIterator<'a> {
    fn new(items: &'a [PulseCode; RMT_BUFFER_LEN]) -> Self {
        // log::info!("RMT ITEMS: {:?}", items);
        Self {
            items,
            pulse_code_value: false,
            pulse_code_quantity: 0,
            bit_idx: 0,
            first_pulse_in_code: true,
        }
    }

    fn read_next_pulse_set(&mut self) {
        match self.first_pulse_in_code {
            true => {
                self.pulse_code_value = self.items[self.bit_idx].level1().into();
                self.pulse_code_quantity = self.items[self.bit_idx].length1();
                if self.pulse_code_quantity % 4 != 0 {
                    log::warn!(
                        "Pulse code quantity is not a multiple of 4: {}",
                        self.pulse_code_quantity
                    );
                }
                self.pulse_code_quantity /= 4;
                self.first_pulse_in_code = false;
            }
            false => {
                self.pulse_code_value = self.items[self.bit_idx].level2().into();
                self.pulse_code_quantity = self.items[self.bit_idx].length2();
                self.first_pulse_in_code = true;
                if self.pulse_code_quantity % 4 != 0 {
                    log::warn!(
                        "Pulse code quantity is not a multiple of 4: {}",
                        self.pulse_code_quantity
                    );
                }
                self.pulse_code_quantity /= 4;
                self.bit_idx += 1;
            }
        }
    }
}

impl<'a> Iterator for RmtBitIterator<'a> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        // Indicates we've sent out all the pulses from the most-recent block
        if self.pulse_code_quantity == 0 {
            // Indicates we're through the buffer, so nothing to follow up with
            if self.bit_idx >= RMT_BUFFER_LEN {
                return None;
            }
            self.read_next_pulse_set();
            // If this is 0 or >= 128 after `read_next_pulse_set` it means
            // the receiver went idle and stopped receiving data.
            // AMC3336 cannot emit pulses >= 128 in a non-error state.
            if self.pulse_code_quantity == 0 || self.pulse_code_quantity >= 128 {
                return None;
            }
        }
        self.pulse_code_quantity -= 1;
        return Some(self.pulse_code_value);
    }
}

#[derive(Clone, Copy, Debug)]
enum SensorStatus {
    On,
    Off,
}

#[allow(non_camel_case_types)]
pub struct Sensors_1_0_0<'a> {
    rmt_channel: Channel<'a, Async, Rx>,
    diag_pin: Input<'a>,
    adc_mux_sel_a: McpOutputPin<'a>,
    adc_mux_sel_b: McpOutputPin<'a>,
    readings: [PulseCode; RMT_BUFFER_LEN],
    adc_clock: LedcChannel<'a, LowSpeed>,
    temp_ec_sel: Output<'a>,
    temp_ec_sqw: Output<'a>,
    sensor_status: SensorStatus,
}

impl<'a> Sensors_1_0_0<'a> {
    pub fn init(
        rmt_channel: Channel<'a, Async, Rx>,
        adc_mux_sel_a: McpOutputPin<'a>,
        adc_mux_sel_b: McpOutputPin<'a>,
        adc_clock: LedcChannel<'a, LowSpeed>,
        temp_ec_sel: Output<'a>,
        temp_ec_sqw: Output<'a>,
        diag_pin: Input<'a>,
    ) -> Self {
        Self {
            rmt_channel,
            diag_pin,
            adc_mux_sel_a,
            adc_mux_sel_b,
            readings: [PulseCode::default(); RMT_BUFFER_LEN],
            adc_clock,
            temp_ec_sel,
            temp_ec_sqw,
            sensor_status: SensorStatus::Off,
        }
    }

    async fn turn_clock_on(&mut self) -> Result<(), SensorReadError> {
        match self.sensor_status {
            SensorStatus::On => Ok(()),
            SensorStatus::Off => {
                self.adc_clock
                    .set_duty(50)
                    .map_err(SensorReadError::ClockStartFailure)?;
                self.sensor_status = SensorStatus::On;
                Timer::after_millis(250).await;
                Ok(())
            }
        }
    }

    async fn turn_clock_off(&mut self) -> Result<(), SensorReadError> {
        match self.sensor_status {
            SensorStatus::On => {
                self.adc_clock
                    .set_duty(0)
                    .map_err(SensorReadError::ClockStartFailure)?;
                self.sensor_status = SensorStatus::Off;
            }
            SensorStatus::Off => (),
        }
        Ok(())
    }

    async fn select_sensor(&mut self, sensor_type: SensorType) -> Result<(), SensorReadError> {
        match sensor_type {
            SensorType::Ph => {
                self.adc_mux_sel_a
                    .set_high()
                    .map_err(SensorReadError::MuxSelect)?;
                self.adc_mux_sel_b
                    .set_low()
                    .map_err(SensorReadError::MuxSelect)?;
            }
            SensorType::Orp => {
                self.adc_mux_sel_a
                    .set_high()
                    .map_err(SensorReadError::MuxSelect)?;
                self.adc_mux_sel_b
                    .set_high()
                    .map_err(SensorReadError::MuxSelect)?;
            }
            SensorType::Conductivity => {
                self.adc_mux_sel_a
                    .set_low()
                    .map_err(SensorReadError::MuxSelect)?;
                self.adc_mux_sel_b
                    .set_high()
                    .map_err(SensorReadError::MuxSelect)?;
            }
            SensorType::Temperature => {
                self.adc_mux_sel_a
                    .set_low()
                    .map_err(SensorReadError::MuxSelect)?;
                self.adc_mux_sel_b
                    .set_high()
                    .map_err(SensorReadError::MuxSelect)?;
                self.temp_ec_sel.set_low();
                self.temp_ec_sqw.set_low();
            }
        };
        // Timer::after(embassy_time::Duration::from_millis(100)).await;
        Ok(())
    }

    fn read_sensor(&mut self) -> Result<(), SensorReadError> {
        for entry in self.readings.iter_mut() {
            *entry = PulseCode::default().into();
        }
        let waker = Waker::noop();
        let mut cx = Context::from_waker(&waker);
        let mut rx_future = pin!(self.rmt_channel.receive(&mut self.readings));
        // log::debug!("Starting RMT Read");
        loop {
            match rx_future.as_mut().poll(&mut cx) {
                Poll::Ready(Ok(_)) => {
                    // log::debug!("Received data");
                    return Ok(());
                }
                Poll::Ready(Err(RmtError::ReceiverError)) => {
                    // log::debug!("RMT buffer exhausted (expected)");
                    return Ok(());
                }
                Poll::Ready(Err(e)) => {
                    return Err(SensorReadError::Rmt(e));
                }
                Poll::Pending => {
                    if self.diag_pin.is_low() {
                        return Err(SensorReadError::IsolatedPowerProblem);
                    }
                }
            }
        }
    }

    async fn read_ph_voltage(&mut self) -> Result<f32, SensorReadError> {
        self.turn_clock_on().await?;
        self.select_sensor(SensorType::Ph).await?;
        self.read_sensor()?;
        self.convert_input_data_to_voltage()
    }

    async fn read_ec_voltage(&mut self) -> Result<f32, SensorReadError> {
        self.turn_clock_on().await?;
        self.select_sensor(SensorType::Conductivity).await?;
        self.temp_ec_sel.set_high();
        self.temp_ec_sqw.set_high();

        let start_time = Instant::now();
        let read_result = self.read_sensor();
        let receive_duration = Instant::now() - start_time;

        debug!("EC read duration: {:?}", receive_duration);
        self.temp_ec_sqw.set_low();
        Timer::after(receive_duration).await;
        self.temp_ec_sel.set_low();

        read_result?;
        self.convert_input_data_to_voltage()
    }

    async fn read_orp_voltage(&mut self) -> Result<f32, SensorReadError> {
        self.turn_clock_on().await?;
        self.select_sensor(SensorType::Orp).await?;
        self.read_sensor()?;
        self.convert_input_data_to_voltage()
    }

    async fn read_temperature_voltage(&mut self) -> Result<f32, SensorReadError> {
        self.turn_clock_on().await?;
        self.select_sensor(SensorType::Temperature).await?;
        self.read_sensor()?;
        self.convert_input_data_to_voltage()
    }

    fn convert_input_data_to_voltage(&self) -> Result<f32, SensorReadError> {
        let val = cic_filter_order_3(&RmtReadings::new(&self.readings), OSR)
            .map_err(|_| SensorReadError::InsufficientData)?;
        let ratio = (val as f32) / OSR_DIVISOR as f32;
        Ok(((ratio * ADC_RANGE) - ADC_RANGE / 2.0) * 1000.0)
    }
}

impl<'a> SensorReadRaw for Sensors_1_0_0<'a> {
    async fn turn_sensors_on(&mut self) -> Result<(), SensorError> {
        self.turn_clock_on()
            .await
            .map_err(|_| SensorError::HardwareControlFailure)
    }

    async fn turn_sensors_off(&mut self) -> Result<(), SensorError> {
        self.turn_clock_off()
            .await
            .map_err(|_| SensorError::HardwareControlFailure)
    }

    async fn read_sensor_raw(&mut self, sensor_type: SensorType) -> Result<f32, SensorError> {
        let result = match sensor_type {
            SensorType::Ph => self.read_ph_voltage().await,
            SensorType::Orp => self.read_orp_voltage().await,
            SensorType::Conductivity => self.read_ec_voltage().await,
            SensorType::Temperature => self.read_temperature_voltage().await,
        };
        match result {
            Ok(voltage) => {
                log::info!("Read voltage for sensor {:?}: {:?}", sensor_type, voltage);
                Ok(voltage)
            }
            Err(e) => {
                log::error!("Sensor read failed for {:?}: {:?}", sensor_type, e);
                Err(SensorError::HardwareReadFailure(sensor_type))
            }
        }
    }

    fn adc_mv_to_sensor_value(&self, sensor_type: SensorType, raw_adc_mv: f32) -> f32 {
        match sensor_type {
            SensorType::Ph => raw_adc_mv,
            SensorType::Orp => {
                // V_Ref goes through a 100k/100k resistor divider
                // Then the output also goes through a 100k/50k divider
                // before connecting to AIN_P to account for an expanded
                // sensor voltage range.
                let sensor_output_voltage = raw_adc_mv * 3.0;
                let divided_vref = VOLTAGE_REFERENCE_MV / 2.0;
                sensor_output_voltage - divided_vref
            }
            SensorType::Conductivity => {
                // V_Ref goes through a 10k precision resistor, then the sensor,
                // then a 1k precision resistor, then to ground.
                raw_adc_mv * 11000.0 / (VOLTAGE_REFERENCE_MV - raw_adc_mv)
            }
            SensorType::Temperature => {
                // Same situation as Conductivity
                raw_adc_mv * 11000.0 / (VOLTAGE_REFERENCE_MV - raw_adc_mv)
            }
        }
    }
}
