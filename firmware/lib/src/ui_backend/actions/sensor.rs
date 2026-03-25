cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::format;
    } else {
        use alloc::format;
    }
}

use core::fmt::Debug;
use embedded_storage::Storage;
use slint::ComponentHandle;

use crate::{
    peripherals::{rtc::RealTimeClock, PumpController, SensorReadRaw},
    storage::update_device_config,
    ui_types::{MainWindow, SensorType, SensorUiState},
};

use super::MessageContext;

#[derive(Debug, Clone)]
pub struct EnableSensor {
    pub sensor_type: SensorType,
}

impl EnableSensor {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<SensorUiState>()
            .on_enable_sensor(move |sensor_type| {
                send(EnableSensor { sensor_type });
            });
    }

    pub async fn handle<
        'a,
        Rtc: RealTimeClock,
        Sensors: SensorReadRaw,
        Pumps: PumpController,
        S: Storage<Error = E>,
        E: Debug,
    >(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        update_device_config(
            ctx.config_buffer,
            ctx.current_timestamp,
            |device_config| match self.sensor_type {
                SensorType::Ph => device_config.ph.enabled = true,
                SensorType::Conductivity => device_config.ec.enabled = true,
                SensorType::Orp => device_config.orp.enabled = true,
                SensorType::Temperature => device_config.temperature.enabled = true,
            },
        )
        .await;
    }
}

#[derive(Debug, Clone)]
pub struct DisableSensor {
    pub sensor_type: SensorType,
}

impl DisableSensor {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<SensorUiState>()
            .on_disable_sensor(move |sensor_type| {
                send(DisableSensor { sensor_type });
            });
    }

    pub async fn handle<
        'a,
        Rtc: RealTimeClock,
        Sensors: SensorReadRaw,
        Pumps: PumpController,
        S: Storage<Error = E>,
        E: Debug,
    >(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        update_device_config(
            ctx.config_buffer,
            ctx.current_timestamp,
            |device_config| match self.sensor_type {
                SensorType::Ph => device_config.ph.enabled = false,
                SensorType::Conductivity => device_config.ec.enabled = false,
                SensorType::Orp => device_config.orp.enabled = false,
                SensorType::Temperature => device_config.temperature.enabled = false,
            },
        )
        .await;
    }
}

#[derive(Debug, Clone)]
pub struct SetSensorMinValue {
    pub sensor_type: SensorType,
    pub min_value: f32,
}

impl SetSensorMinValue {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<SensorUiState>()
            .on_set_sensor_min_value(move |sensor_type, min_value| {
                send(SetSensorMinValue {
                    sensor_type,
                    min_value,
                });
            });
    }

    pub async fn handle<
        'a,
        Rtc: RealTimeClock,
        Sensors: SensorReadRaw,
        Pumps: PumpController,
        S: Storage<Error = E>,
        E: Debug,
    >(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        update_device_config(
            ctx.config_buffer,
            ctx.current_timestamp,
            |device_config| match self.sensor_type {
                SensorType::Ph => device_config.ph.min_acceptable = self.min_value,
                SensorType::Conductivity => device_config.ec.min_acceptable = self.min_value,
                SensorType::Orp => device_config.orp.min_acceptable = self.min_value,
                SensorType::Temperature => {
                    device_config.temperature.min_acceptable = self.min_value
                }
            },
        )
        .await;
    }
}

#[derive(Debug, Clone)]
pub struct SetSensorMaxValue {
    pub sensor_type: SensorType,
    pub max_value: f32,
}

impl SetSensorMaxValue {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<SensorUiState>()
            .on_set_sensor_max_value(move |sensor_type, max_value| {
                send(SetSensorMaxValue {
                    sensor_type,
                    max_value,
                });
            });
    }

    pub async fn handle<
        'a,
        Rtc: RealTimeClock,
        Sensors: SensorReadRaw,
        Pumps: PumpController,
        S: Storage<Error = E>,
        E: Debug,
    >(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        update_device_config(
            ctx.config_buffer,
            ctx.current_timestamp,
            |device_config| match self.sensor_type {
                SensorType::Ph => device_config.ph.max_acceptable = self.max_value,
                SensorType::Conductivity => device_config.ec.max_acceptable = self.max_value,
                SensorType::Orp => device_config.orp.max_acceptable = self.max_value,
                SensorType::Temperature => {
                    device_config.temperature.max_acceptable = self.max_value
                }
            },
        )
        .await;
    }
}

#[derive(Debug, Clone)]
pub struct SetThermistorBeta {
    pub beta_value: f32,
}

impl SetThermistorBeta {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<SensorUiState>()
            .on_set_thermistor_beta(move |beta_value| {
                send(SetThermistorBeta { beta_value });
            });
    }

    pub async fn handle<
        'a,
        Rtc: RealTimeClock,
        Sensors: SensorReadRaw,
        Pumps: PumpController,
        S: Storage<Error = E>,
        E: Debug,
    >(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            device_config.temperature.beta_value = Some(self.beta_value);
        })
        .await;
        crate::ui_backend::state::set_beta_confirmed();
    }
}

#[derive(Debug, Clone)]
pub struct ReadSensorRaw {
    pub sensor_type: SensorType,
}

impl ReadSensorRaw {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<SensorUiState>()
            .on_measure_sensor_raw_value(move |sensor_type| {
                send(ReadSensorRaw { sensor_type });
            });
    }

    pub async fn handle<
        'a,
        Rtc: RealTimeClock,
        Sensors: SensorReadRaw,
        Pumps: PumpController,
        S: Storage<Error = E>,
        E: Debug,
    >(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        let mut tc = ctx.treatment_controller.lock().await;
        let result: Result<f32, _> = match self.sensor_type {
            SensorType::Ph => tc
                .sensor_controller
                .measure_ph_voltage()
                .await
                .map(|v| v.mv()),
            SensorType::Orp => tc
                .sensor_controller
                .measure_orp_voltage()
                .await
                .map(|v| v.mv()),
            SensorType::Temperature => tc
                .sensor_controller
                .measure_temperature_resistance()
                .await
                .map(|r| r.ohms()),
            SensorType::Conductivity => tc
                .sensor_controller
                .measure_ec_resistance()
                .await
                .map(|r| r.ohms()),
        };
        match result {
            Ok(value) => {
                crate::ui_backend::state::update_voltage_reading(self.sensor_type, value).await;
            }
            Err(e) => {
                log::error!("Sensor {:?} raw read failed: {}", self.sensor_type, e);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReadSensorCalibrated {
    pub sensor_type: SensorType,
}

impl ReadSensorCalibrated {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<SensorUiState>()
            .on_measure_sensor_calibrated_value(move |sensor_type| {
                send(ReadSensorCalibrated { sensor_type });
            });
    }

    pub async fn handle<
        'a,
        Rtc: RealTimeClock,
        Sensors: SensorReadRaw,
        Pumps: PumpController,
        S: Storage<Error = E>,
        E: Debug,
    >(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        let config = crate::storage::get_device_config().await;
        let mut tc = ctx.treatment_controller.lock().await;

        match self.sensor_type {
            SensorType::Temperature => {
                let beta = match config.temperature.beta_value {
                    Some(beta) => beta,
                    None => {
                        log::error!("Temperature read failed: no beta value configured");
                        return;
                    }
                };
                match tc.sensor_controller.measure_temperature(beta).await {
                    Ok(temp) => {
                        crate::ui_backend::state::update_manual_sensor_reading(
                            self.sensor_type,
                            format!("{:.1}°C", temp.celsius()).into(),
                        )
                        .await;
                    }
                    Err(e) => log::error!("Temperature read failed in sensor value command: {}", e),
                }
            }
            SensorType::Ph => {
                let default_cal = crate::config::calibration::ThreePointPhCalibration::default();
                let ph_cal = match config.ph.calibration.as_ref() {
                    Some(cal) => cal,
                    None => {
                        log::warn!("No pH calibration configured, using default");
                        &default_cal
                    }
                };
                let temperature = match config.temperature.beta_value {
                    Some(beta) => tc
                        .sensor_controller
                        .measure_temperature(beta)
                        .await
                        .unwrap_or_default(),
                    None => crate::units::Temperature::default(),
                };
                match tc.sensor_controller.measure_ph(ph_cal, temperature).await {
                    Ok(measurement) => {
                        crate::ui_backend::state::update_manual_sensor_reading(
                            self.sensor_type,
                            format!("{:.2} pH", measurement.ph_value).into(),
                        )
                        .await;
                    }
                    Err(e) => log::error!("pH read failed in calibrated value command: {}", e),
                }
            }
            SensorType::Orp => {
                let default_cal = crate::config::calibration::OrpCalibration::default();
                let orp_cal = match config.orp.calibration.as_ref() {
                    Some(cal) => cal,
                    None => {
                        log::warn!("No ORP calibration configured, using default");
                        &default_cal
                    }
                };
                match tc.sensor_controller.measure_orp(orp_cal).await {
                    Ok(measurement) => {
                        crate::ui_backend::state::update_manual_sensor_reading(
                            self.sensor_type,
                            format!("{:.0}mV", measurement.voltage.mv()).into(),
                        )
                        .await;
                    }
                    Err(e) => log::error!("ORP read failed in calibrated value command: {}", e),
                }
            }
            SensorType::Conductivity => {
                let default_cal = crate::config::calibration::ConductivityCalibration::default();
                let ec_cal = match config.ec.calibration.as_ref() {
                    Some(cal) => cal,
                    None => {
                        log::warn!("No EC calibration configured, using default");
                        &default_cal
                    }
                };
                let temperature = match config.temperature.beta_value {
                    Some(beta) => tc
                        .sensor_controller
                        .measure_temperature(beta)
                        .await
                        .unwrap_or_default(),
                    None => crate::units::Temperature::default(),
                };
                match tc
                    .sensor_controller
                    .measure_conductivity(ec_cal, temperature)
                    .await
                {
                    Ok(measurement) => {
                        crate::ui_backend::state::update_manual_sensor_reading(
                            self.sensor_type,
                            format!("{:.0} µS/cm", measurement.us_per_cm()).into(),
                        )
                        .await;
                    }
                    Err(e) => log::error!("EC read failed in calibrated value command: {}", e),
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct FetchSensorChart {
    pub sensor_type: SensorType,
}

impl FetchSensorChart {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<SensorUiState>()
            .on_view_history(move |sensor_type| {
                send(FetchSensorChart { sensor_type });
            });
    }

    pub async fn handle<
        'a,
        Rtc: RealTimeClock,
        Sensors: SensorReadRaw,
        Pumps: PumpController,
        S: Storage<Error = E>,
        E: Debug,
    >(
        self,
        _ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        crate::ui_backend::state::request_chart(self.sensor_type);
    }
}

#[derive(Debug, Clone)]
pub struct MeasureAndCalibrateEc {
    pub solution_us_per_cm: f32,
}

impl MeasureAndCalibrateEc {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<crate::ui_types::WorkflowUiState>()
            .on_trigger_ec_calibration(move |solution_us_per_cm| {
                send(MeasureAndCalibrateEc { solution_us_per_cm });
            });
    }

    pub async fn handle<
        'a,
        Rtc: RealTimeClock,
        Sensors: SensorReadRaw,
        Pumps: PumpController,
        S: Storage<Error = E>,
        E: Debug,
    >(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        use crate::config::calibration::ConductivityCalibration;
        use crate::units::Temperature;

        let beta = match crate::storage::get_device_config()
            .await
            .temperature
            .beta_value
        {
            Some(beta) => beta,
            None => {
                log::error!("EC calibration: no beta value configured");
                return;
            }
        };

        let mut tc = ctx.treatment_controller.lock().await;
        let temperature = match tc.sensor_controller.measure_temperature(beta).await {
            Ok(t) => t,
            Err(e) => {
                log::error!("EC calibration: temperature read failed: {}", e);
                Temperature::default()
            }
        };
        match tc.sensor_controller.measure_ec_resistance().await {
            Ok(resistance) => {
                let calibration = ConductivityCalibration::new(
                    resistance,
                    self.solution_us_per_cm,
                    temperature,
                    ctx.current_timestamp,
                );
                drop(tc);
                update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
                    device_config.ec.calibration = Some(calibration);
                })
                .await;
                log::info!(
                    "EC calibration saved: K={:.3} cm⁻¹ ({:.0} µS/cm, {:.1}°C)",
                    calibration.cell_constant,
                    self.solution_us_per_cm,
                    temperature.celsius()
                );
            }
            Err(e) => {
                log::error!("EC calibration: resistance read failed: {}", e);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct MeasureAndCalibrateOrp {
    pub solution_mv: f32,
}

impl MeasureAndCalibrateOrp {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<crate::ui_types::WorkflowUiState>()
            .on_trigger_orp_calibration(move |solution_mv| {
                send(MeasureAndCalibrateOrp { solution_mv });
            });
    }

    pub async fn handle<
        'a,
        Rtc: RealTimeClock,
        Sensors: SensorReadRaw,
        Pumps: PumpController,
        S: Storage<Error = E>,
        E: Debug,
    >(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        use crate::config::calibration::OrpCalibration;
        use crate::units::Voltage;

        let mut tc = ctx.treatment_controller.lock().await;
        match tc.sensor_controller.measure_orp_voltage().await {
            Ok(measured_voltage) => {
                let solution_voltage = Voltage::from_mv(self.solution_mv);
                let calibration =
                    OrpCalibration::new(measured_voltage, solution_voltage, ctx.current_timestamp);
                drop(tc);
                update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
                    device_config.orp.calibration = Some(calibration);
                })
                .await;
                log::info!(
                    "ORP calibration saved: {:.0} mV measured, {:.0} mV solution",
                    measured_voltage.mv(),
                    self.solution_mv
                );
            }
            Err(e) => {
                log::error!("ORP calibration: voltage read failed: {}", e);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SavePhCalibration {
    pub low_ph: f32,
    pub low_mv: f32,
    pub low_temp_c: f32,
    pub mid_ph: f32,
    pub mid_mv: f32,
    pub mid_temp_c: f32,
    pub high_ph: f32,
    pub high_mv: f32,
    pub high_temp_c: f32,
}

impl SavePhCalibration {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<crate::ui_types::WorkflowUiState>()
            .on_save_ph_calibration(
                move |low_ph,
                      low_mv,
                      low_temp_c,
                      mid_ph,
                      mid_mv,
                      mid_temp_c,
                      high_ph,
                      high_mv,
                      high_temp_c| {
                    send(SavePhCalibration {
                        low_ph,
                        low_mv,
                        low_temp_c,
                        mid_ph,
                        mid_mv,
                        mid_temp_c,
                        high_ph,
                        high_mv,
                        high_temp_c,
                    });
                },
            );
    }

    pub async fn handle<
        'a,
        Rtc: RealTimeClock,
        Sensors: SensorReadRaw,
        Pumps: PumpController,
        S: Storage<Error = E>,
        E: Debug,
    >(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        use crate::config::calibration::{PhMeasurementPoint, ThreePointPhCalibration};
        use crate::units::{Temperature, Voltage};

        let low_point = PhMeasurementPoint::new(
            self.low_ph,
            Voltage::from_mv(self.low_mv),
            Temperature::from_celsius(self.low_temp_c),
        );
        let mid_point = PhMeasurementPoint::new(
            self.mid_ph,
            Voltage::from_mv(self.mid_mv),
            Temperature::from_celsius(self.mid_temp_c),
        );
        let high_point = PhMeasurementPoint::new(
            self.high_ph,
            Voltage::from_mv(self.high_mv),
            Temperature::from_celsius(self.high_temp_c),
        );

        let calibration =
            ThreePointPhCalibration::new(high_point, mid_point, low_point, ctx.current_timestamp);

        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            device_config.ph.calibration = Some(calibration);
        })
        .await;
    }
}
