cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::{string::String, vec::Vec, string::ToString};
    } else {
        use alloc::{string::String, vec::Vec, string::ToString, format};
    }
}

use chrono::{DateTime, Utc};
use chrono_tz::Tz;

use slint::ComponentHandle;

use crate::{
    config::calibration::{
        types::TimestampedValue, ConductivityCalibration, OrpCalibration, ThreePointPhCalibration,
    },
    peripherals::{DosingPump, DosingPumpStateList, OutletStateList},
    ui_types::{
        ConductivityDisplayUnit, DosingPumpUiState, MainWindow, Outlet, OutletUiState, PumpUiState,
        SensorType, SensorUiState, Status, TemperatureDisplayUnit,
    },
    units::Volume,
};

#[derive(Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct LogEntry {
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

impl TimestampedValue for LogEntry {
    fn get_written_timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct PhSensorState {
    pub enabled: bool,
    pub calibration: Option<ThreePointPhCalibration>,
    pub status: Status,
    pub error_code: Option<String>,
    pub min_acceptable: f32,
    pub max_acceptable: f32,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct EcSensorState {
    pub enabled: bool,
    pub calibration: Option<ConductivityCalibration>,
    pub status: Status,
    pub error_code: Option<String>,
    pub min_acceptable: f32,
    pub max_acceptable: f32,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct OrpSensorState {
    pub enabled: bool,
    pub calibration: Option<OrpCalibration>,
    pub status: Status,
    pub error_code: Option<String>,
    pub min_acceptable: f32,
    pub max_acceptable: f32,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct TemperatureSensorState {
    pub enabled: bool,
    pub status: Status,
    pub beta_value: Option<f32>,
    pub error_code: Option<String>,
    pub min_acceptable: f32,
    pub max_acceptable: f32,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, PartialEq, Debug)]
pub enum ClockFormat {
    TwentyFourHour,
    TwelveHour,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, PartialEq, Debug)]
pub struct TimeDisplayConfig {
    pub timezone: Tz,
    pub format: ClockFormat,
}

impl TimeDisplayConfig {
    pub fn convert_to_local(&self, utc_time: DateTime<Utc>) -> DateTime<Tz> {
        utc_time.with_timezone(&self.timezone)
    }

    pub fn format_date(&self, utc_time: DateTime<Utc>) -> String {
        let local_time = self.convert_to_local(utc_time);
        local_time.format("%b %-d").to_string()
    }

    pub fn format_time(&self, utc_time: DateTime<Utc>) -> String {
        let local_time = self.convert_to_local(utc_time);
        match self.format {
            ClockFormat::TwentyFourHour => local_time.format("%H:%M").to_string(),
            ClockFormat::TwelveHour => local_time.format("%-I:%M%P").to_string(),
        }
    }

    pub fn format_datetime(&self, utc_time: DateTime<Utc>) -> String {
        let date = self.format_date(utc_time);
        let time = self.format_time(utc_time);
        format!("{} {}", date, time)
    }

    pub fn timezone_display_name(&self) -> &str {
        self.timezone.name()
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct DeviceConfig {
    // This is probably not ideal, but rather than write a million
    // getters and setters the fields are just gonna be public.
    // If you're changing or reading a field then the type system
    // should have you covered, unless we do something stupid like
    // make two fields with identical types, in which case
    // it is possible to do something funky and make this public
    pub tank_size: Volume,
    pub temperature_display_unit: TemperatureDisplayUnit,
    pub conductivity_display_unit: ConductivityDisplayUnit,
    pub time_display_config: TimeDisplayConfig,
    pub configuration_written_at: DateTime<Utc>,
    pub ph: PhSensorState,
    pub ec: EcSensorState,
    pub orp: OrpSensorState,
    pub temperature: TemperatureSensorState,
    pub pumps: DosingPumpStateList,
    pub outlets: OutletStateList,
}

impl TimestampedValue for DeviceConfig {
    fn get_written_timestamp(&self) -> DateTime<Utc> {
        self.configuration_written_at
    }
}

impl DeviceConfig {
    pub const fn default() -> DeviceConfig {
        DeviceConfig {
            tank_size: Volume::from_liters(0.0),
            temperature_display_unit: TemperatureDisplayUnit::Celsius,
            conductivity_display_unit: ConductivityDisplayUnit::UsPerCm,
            time_display_config: TimeDisplayConfig {
                timezone: Tz::UTC,
                format: ClockFormat::TwelveHour,
            },
            configuration_written_at: DateTime::<Utc>::from_timestamp_millis(0).unwrap(),
            ph: PhSensorState {
                enabled: false,
                calibration: None,
                status: Status::Ok,
                error_code: None,
                min_acceptable: 5.5,
                max_acceptable: 7.5,
            },
            ec: EcSensorState {
                enabled: false,
                calibration: None,
                status: Status::Ok,
                error_code: None,
                min_acceptable: 800.0,
                max_acceptable: 1800.0,
            },
            orp: OrpSensorState {
                enabled: false,
                calibration: None,
                status: Status::Ok,
                error_code: None,
                min_acceptable: 400.0,
                max_acceptable: 800.0,
            },
            temperature: TemperatureSensorState {
                enabled: true,
                status: Status::Ok,
                beta_value: None,
                error_code: None,
                min_acceptable: 18.0,
                max_acceptable: 28.0,
            },
            pumps: DosingPumpStateList::default(),
            outlets: OutletStateList::default(),
        }
    }

    pub fn update_ui_dosing_pump_state(&mut self, ui: &MainWindow, pump: DosingPump) {
        let pump_state = self.pumps.get_dosing_pump_state(pump);
        let ui_state = DosingPumpUiState::from(pump_state);
        let pump_index = pump.to_int();

        let pump_config = ui.global::<PumpUiState>();

        let mut all_pump_states = Vec::new();
        for i in 0..6 {
            if i == pump_index {
                all_pump_states.push(ui_state.clone());
            } else {
                let other_pump = DosingPump::from_int(i).unwrap();
                let other_state = self.pumps.get_dosing_pump_state(other_pump);
                all_pump_states.push(DosingPumpUiState::from(other_state));
            }
        }

        pump_config.set_dosing_pump_states(all_pump_states.as_slice().into());
    }

    pub fn update_ui_outlet_state(&mut self, ui: &MainWindow, outlet: Outlet) {
        let outlet_state = self.outlets.get_outlet_state(outlet);
        let ui_state = OutletUiState::from(outlet_state);
        let outlet_index = outlet.to_int();

        let pump_config = ui.global::<PumpUiState>();

        let mut all_outlet_states = Vec::new();
        for i in 0..3 {
            if i == outlet_index {
                all_outlet_states.push(ui_state.clone());
            } else {
                let other_outlet = Outlet::from_int(i).unwrap();
                let other_state = self.outlets.get_outlet_state(other_outlet);
                all_outlet_states.push(OutletUiState::from(other_state));
            }
        }

        pump_config.set_configurable_outlet_states(all_outlet_states.as_slice().into());
    }

    pub fn update_ui_sensor_state(&self, ui: &MainWindow, sensor_type: SensorType) {
        let sensor_config = ui.global::<SensorUiState>();
        match sensor_type {
            SensorType::Ph => {
                sensor_config.set_ph_enabled(self.ph.enabled);
                sensor_config.set_ph_status(self.ph.status);
                sensor_config.set_ph_min_acceptable(self.ph.min_acceptable);
                sensor_config.set_ph_max_acceptable(self.ph.max_acceptable);

                // Update pH calibration slope if available
                if let Some(ph_calibration) = &self.ph.calibration {
                    let workflow_state = ui.global::<crate::ui_types::WorkflowUiState>();
                    workflow_state.set_ph_calibration_slope(ph_calibration.slope_percentage());
                }
            }
            SensorType::Conductivity => {
                sensor_config.set_ec_enabled(self.ec.enabled);
                sensor_config.set_ec_status(self.ec.status);
                sensor_config.set_ec_min_acceptable(self.ec.min_acceptable);
                sensor_config.set_ec_max_acceptable(self.ec.max_acceptable);
            }
            SensorType::Orp => {
                sensor_config.set_orp_enabled(self.orp.enabled);
                sensor_config.set_orp_status(self.orp.status);
                sensor_config.set_orp_min_acceptable(self.orp.min_acceptable);
                sensor_config.set_orp_max_acceptable(self.orp.max_acceptable);
            }
            SensorType::Temperature => {
                sensor_config.set_temperature_enabled(self.temperature.enabled);
                sensor_config.set_temperature_status(self.temperature.status);
                sensor_config.set_temperature_min_acceptable(self.temperature.min_acceptable);
                sensor_config.set_temperature_max_acceptable(self.temperature.max_acceptable);
            }
        }
    }

    pub fn update_ui_app_config(&self, ui: &MainWindow) {
        let app_config = ui.global::<crate::ui_types::AppUiState>();
        app_config.set_tank_size(self.tank_size.to_liters());
        app_config.set_temperature_display_unit(self.temperature_display_unit);
        app_config.set_conductivity_display_unit(self.conductivity_display_unit);
    }

    pub fn update_current_time(&self, ui: &MainWindow, current_utc_time: DateTime<Utc>) {
        let app_config = ui.global::<crate::ui_types::AppUiState>();
        let formatted_time = self.time_display_config.format_datetime(current_utc_time);
        app_config.set_current_time(formatted_time.into());
    }

    pub fn populate_ui_from_backend(&mut self, ui: &MainWindow) {
        for sensor_type in [
            SensorType::Ph,
            SensorType::Conductivity,
            SensorType::Orp,
            SensorType::Temperature,
        ] {
            self.update_ui_sensor_state(ui, sensor_type);
        }

        self.update_ui_app_config(ui);

        for i in 0..6 {
            let pump = DosingPump::from_int(i).unwrap();
            self.update_ui_dosing_pump_state(ui, pump);
        }
        for i in 0..3 {
            let outlet = Outlet::from_int(i).unwrap();
            self.update_ui_outlet_state(ui, outlet);
        }
    }
}
