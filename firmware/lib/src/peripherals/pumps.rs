cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::format;
    } else {
        use alloc::format;
    }
}
use core::marker::PhantomData;
use slint::SharedString;

use ads1x1x::{
    channel,
    ic::{Ads1015, Resolution12Bit},
    mode::OneShot,
    Ads1x1x, TargetAddr,
};

use embassy_time::Timer;
use thiserror::Error;

use crate::{
    config::calibration::DosingPumpCalibration,
    ui_types::{DosingPump, Outlet, Pump, Status, TreatmentSolutionType, UiTreatmentSolution},
};

#[allow(async_fn_in_trait)]
pub trait PumpController {
    async fn enable_pump(&mut self, pump: &Pump) -> Result<(), PumpError>;
    async fn disable_pump(&mut self, pump: &Pump) -> Result<(), PumpError>;
    async fn read_current(&mut self, pump: &Pump) -> Result<f32, PumpError>;
    async fn turn_off_all(&mut self) -> Result<(), PumpError>;
    fn is_pump_enabled(&mut self, pump: &Pump) -> Result<bool, PumpError>;
    fn enable_relay(&mut self);
    fn kill_relay(&mut self);
}

pub const CURRENT_CUTOFF: f32 = 0.05;

const SHUNT_RESISTOR_VALUE: f32 = 0.05;

#[derive(Error, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PumpError {
    #[error("No current detected when expected")]
    NoCurrent,

    #[error("Unexpected current flowing")]
    UnexpectedCurrent,

    #[error("Pump hardware communication failure")]
    HardwareCommunication,
}

impl DosingPump {
    pub fn to_int(&self) -> usize {
        match self {
            DosingPump::DoseOne => 0,
            DosingPump::DoseTwo => 1,
            DosingPump::DoseThree => 2,
            DosingPump::DoseFour => 3,
            DosingPump::DoseFive => 4,
            DosingPump::DoseSix => 5,
        }
    }

    pub fn from_int(int: usize) -> Option<Self> {
        match int {
            0 => Some(DosingPump::DoseOne),
            1 => Some(DosingPump::DoseTwo),
            2 => Some(DosingPump::DoseThree),
            3 => Some(DosingPump::DoseFour),
            4 => Some(DosingPump::DoseFive),
            5 => Some(DosingPump::DoseSix),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct DosingPumpState {
    pub pump: DosingPump,
    pub name: Option<SharedString>,
    pub status: Status,
    pub enabled: bool,
    pub treatment_solution: UiTreatmentSolution,
    pub calibration: DosingPumpCalibration,
}

impl DosingPumpState {
    pub const fn default(pump: DosingPump) -> Self {
        Self {
            pump,
            name: None,
            status: Status::Ok,
            enabled: false,
            treatment_solution: UiTreatmentSolution {
                solution_type: TreatmentSolutionType::Unconfigured,
                solution_strength: 0.0,
            },
            calibration: DosingPumpCalibration::default(),
        }
    }

    pub fn get_label(&self) -> SharedString {
        match &self.name {
            Some(set_name) => set_name.clone(),
            None => SharedString::from(format!("Pump {}", self.pump.to_int())),
        }
    }

    pub fn calibration(&self) -> &DosingPumpCalibration {
        &self.calibration
    }

    pub fn show_status(&self) -> SharedString {
        match self.enabled {
            false => SharedString::from("Disabled"),
            true => SharedString::from(format!("Status: {:?}", self.status)),
        }
    }
}

pub enum InvalidConfiguration {
    ImproperlyPlacedPump,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct DosingPumpStateList {
    dosing_pumps: [DosingPumpState; 6],
}

impl DosingPumpStateList {
    pub const fn default() -> Self {
        Self {
            dosing_pumps: [
                DosingPumpState::default(DosingPump::DoseOne),
                DosingPumpState::default(DosingPump::DoseTwo),
                DosingPumpState::default(DosingPump::DoseThree),
                DosingPumpState::default(DosingPump::DoseFour),
                DosingPumpState::default(DosingPump::DoseFive),
                DosingPumpState::default(DosingPump::DoseSix),
            ],
        }
    }

    pub fn check_validity(&self) -> Result<(), InvalidConfiguration> {
        for (i, dosing_pump) in self.dosing_pumps.iter().enumerate() {
            if dosing_pump.pump.to_int() != i {
                return Err(InvalidConfiguration::ImproperlyPlacedPump);
            }
        }
        Ok(())
    }

    pub fn get_ph_up_pump(&self) -> Option<&DosingPumpState> {
        self.dosing_pumps
            .iter()
            .find(|pump| pump.treatment_solution.solution_type == TreatmentSolutionType::PhUp)
    }

    pub fn get_ph_down_pump(&self) -> Option<&DosingPumpState> {
        self.dosing_pumps
            .iter()
            .find(|pump| pump.treatment_solution.solution_type == TreatmentSolutionType::PhDown)
    }

    pub fn get_nutrient_pump(&self) -> Option<&DosingPumpState> {
        self.dosing_pumps
            .iter()
            .find(|pump| pump.treatment_solution.solution_type == TreatmentSolutionType::Nutrient)
    }

    pub fn get_orp_pump(&self) -> Option<&DosingPumpState> {
        self.dosing_pumps.iter().find(|pump| {
            pump.treatment_solution.solution_type == TreatmentSolutionType::OrpTreatment
        })
    }

    pub fn get_dosing_pump_state(&self, pump: DosingPump) -> DosingPumpState {
        self.dosing_pumps[pump.to_int()].clone()
    }

    pub fn get_dosing_pump_state_mut(&mut self, pump: DosingPump) -> &mut DosingPumpState {
        &mut self.dosing_pumps[pump.to_int()]
    }
}

pub struct HardwarePumpController<'a, DosePin, OutletPin, I2c, MuxPin, RelayPin> {
    dose_pins: [DosePin; 6],
    outlet_pins: [OutletPin; 4],
    current_sense_adc: Ads1x1x<I2c, Ads1015, Resolution12Bit, OneShot>,
    current_sense_mux_a: MuxPin,
    current_sense_mux_b: MuxPin,
    relay_pin: RelayPin,
    _phantom_lifetime: PhantomData<&'a str>,
}

impl<
        'a,
        DosePin: embedded_hal::digital::StatefulOutputPin,
        OutletPin: embedded_hal::digital::StatefulOutputPin,
        I2c: embedded_hal::i2c::I2c,
        MuxPin: embedded_hal::digital::OutputPin,
        RelayPin: embedded_hal::digital::OutputPin<Error = core::convert::Infallible>,
    > HardwarePumpController<'a, DosePin, OutletPin, I2c, MuxPin, RelayPin>
{
    pub async fn new(
        dose_pins: [DosePin; 6],
        outlet_pins: [OutletPin; 4],
        current_sense_adc_i2c: I2c,
        current_sense_mux_a: MuxPin,
        current_sense_mux_b: MuxPin,
        relay_pin: RelayPin,
    ) -> Result<Self, PumpError> {
        let mut current_sense_adc = Ads1x1x::new_ads1015(current_sense_adc_i2c, TargetAddr::Gnd);

        current_sense_adc
            .set_data_rate(ads1x1x::DataRate12Bit::Sps3300)
            .map_err(|_| PumpError::HardwareCommunication)?;
        current_sense_adc
            .set_full_scale_range(ads1x1x::FullScaleRange::Within2_048V)
            .map_err(|_| PumpError::HardwareCommunication)?;

        Ok(Self {
            dose_pins,
            outlet_pins,
            current_sense_adc,
            current_sense_mux_a,
            current_sense_mux_b,
            relay_pin,
            _phantom_lifetime: PhantomData,
        })
    }

    pub async fn turn_off(&mut self) -> Result<(), PumpError> {
        for pin in &mut self.dose_pins {
            pin.set_low()
                .map_err(|_| PumpError::HardwareCommunication)?;
        }
        for pin in &mut self.outlet_pins {
            pin.set_low()
                .map_err(|_| PumpError::HardwareCommunication)?;
        }
        crate::ui_backend::state::clear_all_pump_states().await;
        Ok(())
    }

    async fn _enable_pump(&mut self, pump: &Pump) -> Result<(), PumpError> {
        match pump {
            Pump::Dose(DosingPump::DoseOne) => self.dose_pins[0]
                .set_high()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Dose(DosingPump::DoseTwo) => self.dose_pins[1]
                .set_high()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Dose(DosingPump::DoseThree) => self.dose_pins[2]
                .set_high()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Dose(DosingPump::DoseFour) => self.dose_pins[3]
                .set_high()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Dose(DosingPump::DoseFive) => self.dose_pins[4]
                .set_high()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Dose(DosingPump::DoseSix) => self.dose_pins[5]
                .set_high()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Cfg(Outlet::One) => self.outlet_pins[0]
                .set_high()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Cfg(Outlet::Two) => self.outlet_pins[1]
                .set_high()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Cfg(Outlet::Three) => self.outlet_pins[2]
                .set_high()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Cfg(Outlet::Four) => self.outlet_pins[3]
                .set_high()
                .map_err(|_| PumpError::HardwareCommunication)?,
        }
        crate::ui_backend::state::set_pump_active(pump, true).await;
        Ok(())
    }

    async fn _disable_pump(&mut self, pump: &Pump) -> Result<(), PumpError> {
        match pump {
            Pump::Dose(DosingPump::DoseOne) => self.dose_pins[0]
                .set_low()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Dose(DosingPump::DoseTwo) => self.dose_pins[1]
                .set_low()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Dose(DosingPump::DoseThree) => self.dose_pins[2]
                .set_low()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Dose(DosingPump::DoseFour) => self.dose_pins[3]
                .set_low()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Dose(DosingPump::DoseFive) => self.dose_pins[4]
                .set_low()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Dose(DosingPump::DoseSix) => self.dose_pins[5]
                .set_low()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Cfg(Outlet::One) => self.outlet_pins[0]
                .set_low()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Cfg(Outlet::Two) => self.outlet_pins[1]
                .set_low()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Cfg(Outlet::Three) => self.outlet_pins[2]
                .set_low()
                .map_err(|_| PumpError::HardwareCommunication)?,
            Pump::Cfg(Outlet::Four) => self.outlet_pins[3]
                .set_low()
                .map_err(|_| PumpError::HardwareCommunication)?,
        }
        crate::ui_backend::state::set_pump_active(pump, false).await;
        Ok(())
    }

    pub fn convert_12_bit_result_to_current(&self, adc_result: i16) -> f32 {
        let converted_voltage = adc_result as f32 * (0.256 / 2048.0);
        converted_voltage / SHUNT_RESISTOR_VALUE
    }

    fn select_current_mux(&mut self, outlet: &Outlet) -> Result<(), PumpError> {
        match outlet {
            Outlet::One => {
                self.current_sense_mux_a
                    .set_high()
                    .map_err(|_| PumpError::HardwareCommunication)?;
                self.current_sense_mux_b
                    .set_high()
                    .map_err(|_| PumpError::HardwareCommunication)?;
            }
            Outlet::Two => {
                self.current_sense_mux_a
                    .set_high()
                    .map_err(|_| PumpError::HardwareCommunication)?;
                self.current_sense_mux_b
                    .set_low()
                    .map_err(|_| PumpError::HardwareCommunication)?;
            }
            Outlet::Three => {
                self.current_sense_mux_a
                    .set_low()
                    .map_err(|_| PumpError::HardwareCommunication)?;
                self.current_sense_mux_b
                    .set_high()
                    .map_err(|_| PumpError::HardwareCommunication)?;
            }
            Outlet::Four => {
                self.current_sense_mux_a
                    .set_low()
                    .map_err(|_| PumpError::HardwareCommunication)?;
                self.current_sense_mux_b
                    .set_low()
                    .map_err(|_| PumpError::HardwareCommunication)?;
            }
        }
        Ok(())
    }

    async fn _read_current_raw(
        &mut self,
        pump: &Pump,
    ) -> Result<i16, nb::Error<ads1x1x::Error<<I2c as embedded_hal::i2c::ErrorType>::Error>>> {
        match pump {
            Pump::Dose(_) => self.current_sense_adc.read(channel::DifferentialA2A3),
            Pump::Cfg(_) => self.current_sense_adc.read(channel::DifferentialA0A1),
        }
    }

    async fn _read_current(&mut self, pump: &Pump) -> Result<f32, PumpError> {
        if let Pump::Cfg(outlet) = pump {
            self.select_current_mux(outlet)?;
        }
        let adc_result = match self._read_current_raw(pump).await {
            Ok(res) => res,
            Err(nb::Error::WouldBlock) => {
                Timer::after_micros(500).await;
                match self._read_current_raw(pump).await {
                    Ok(res) => res,
                    Err(e) => {
                        log::error!("Error re-reading current: {:?}", e);
                        return Err(PumpError::HardwareCommunication);
                    }
                }
            }
            Err(e) => {
                log::error!("Unexpected error reading ADC: {:?}", e);
                return Err(PumpError::HardwareCommunication);
            }
        };
        Ok(self.convert_12_bit_result_to_current(adc_result))
    }
}

impl<
        'a,
        DosePin: embedded_hal::digital::StatefulOutputPin,
        OutletPin: embedded_hal::digital::StatefulOutputPin,
        I2c: embedded_hal::i2c::I2c,
        MuxPin: embedded_hal::digital::OutputPin,
        RelayPin: embedded_hal::digital::OutputPin<Error = core::convert::Infallible>,
    > PumpController for HardwarePumpController<'a, DosePin, OutletPin, I2c, MuxPin, RelayPin>
{
    async fn enable_pump(&mut self, pump: &Pump) -> Result<(), PumpError> {
        self._enable_pump(pump).await
    }

    async fn disable_pump(&mut self, pump: &Pump) -> Result<(), PumpError> {
        self._disable_pump(pump).await
    }

    async fn read_current(&mut self, pump: &Pump) -> Result<f32, PumpError> {
        self._read_current(pump).await
    }

    async fn turn_off_all(&mut self) -> Result<(), PumpError> {
        self.turn_off().await
    }

    fn is_pump_enabled(&mut self, pump: &Pump) -> Result<bool, PumpError> {
        match pump {
            Pump::Dose(DosingPump::DoseOne) => self.dose_pins[0]
                .is_set_high()
                .map_err(|_| PumpError::HardwareCommunication),
            Pump::Dose(DosingPump::DoseTwo) => self.dose_pins[1]
                .is_set_high()
                .map_err(|_| PumpError::HardwareCommunication),
            Pump::Dose(DosingPump::DoseThree) => self.dose_pins[2]
                .is_set_high()
                .map_err(|_| PumpError::HardwareCommunication),
            Pump::Dose(DosingPump::DoseFour) => self.dose_pins[3]
                .is_set_high()
                .map_err(|_| PumpError::HardwareCommunication),
            Pump::Dose(DosingPump::DoseFive) => self.dose_pins[4]
                .is_set_high()
                .map_err(|_| PumpError::HardwareCommunication),
            Pump::Dose(DosingPump::DoseSix) => self.dose_pins[5]
                .is_set_high()
                .map_err(|_| PumpError::HardwareCommunication),
            Pump::Cfg(Outlet::One) => self.outlet_pins[0]
                .is_set_high()
                .map_err(|_| PumpError::HardwareCommunication),
            Pump::Cfg(Outlet::Two) => self.outlet_pins[1]
                .is_set_high()
                .map_err(|_| PumpError::HardwareCommunication),
            Pump::Cfg(Outlet::Three) => self.outlet_pins[2]
                .is_set_high()
                .map_err(|_| PumpError::HardwareCommunication),
            Pump::Cfg(Outlet::Four) => self.outlet_pins[3]
                .is_set_high()
                .map_err(|_| PumpError::HardwareCommunication),
        }
    }

    // Safe to unwrap: RelayPin is constrained to Error = Infallible
    #[allow(clippy::unwrap_used)]
    fn enable_relay(&mut self) {
        self.relay_pin.set_high().unwrap();
    }

    // Safe to unwrap: RelayPin is constrained to Error = Infallible
    #[allow(clippy::unwrap_used)]
    fn kill_relay(&mut self) {
        self.relay_pin.set_low().unwrap();
    }
}
