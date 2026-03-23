pub mod actions;
pub mod callbacks;
pub mod chart;
pub mod state;
pub mod ui_runner;

use slint::Model;
use crate::ui_types::MainWindow;
use crate::logging::LogCategory;

use slint::ComponentHandle;

cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::vec::Vec;
    } else {
        use alloc::vec::Vec;
    }
}

pub fn register_ui_callbacks(ui: &MainWindow) {
    callbacks::register_ui_callbacks(ui);
}

pub async fn sync_runtime_state_to_ui(ui: &MainWindow) {
    let current_readings = crate::ui_backend::state::get_current_sensor_readings().await;
    let voltage_readings = crate::ui_backend::state::get_voltage_readings().await;
    let manual_readings = crate::ui_backend::state::get_manual_sensor_readings().await;
    let pump_state = crate::ui_backend::state::get_pump_runtime_state().await;

    let pump_ui_state = ui.global::<crate::ui_types::PumpUiState>();

    let current_dosing_running = pump_ui_state.get_dosing_pump_running();
    let dosing_changed = pump_state.dosing_pumps.iter().enumerate().any(|(i, &running)| {
        current_dosing_running.row_data(i) != Some(running)
    });
    if dosing_changed {
        pump_ui_state.set_dosing_pump_running(pump_state.dosing_pumps.as_slice().into());
    }

    let current_outlet_running = pump_ui_state.get_outlet_running();
    let outlets_changed = pump_state.outlets.iter().enumerate().any(|(i, &running)| {
        current_outlet_running.row_data(i) != Some(running)
    });
    if outlets_changed {
        pump_ui_state.set_outlet_running(pump_state.outlets.as_slice().into());
    }

    let sensor_ui_state = ui.global::<crate::ui_types::SensorUiState>();

    if let Some(ph) = current_readings.ph_value {
        sensor_ui_state.set_current_ph_value(ph);
    }
    if let Some(ec) = current_readings.ec_value {
        sensor_ui_state.set_current_ec_value(ec);
    }
    if let Some(orp) = current_readings.orp_value {
        sensor_ui_state.set_current_orp_value(orp);
    }
    if let Some(temp) = current_readings.temperature_celsius {
        sensor_ui_state.set_current_temperature_celsius(temp);
    }

    if let Some(mv) = voltage_readings.ph_mv {
        sensor_ui_state.set_latest_ph_voltage_mv(mv);
    }
    if let Some(mv) = voltage_readings.ec_mv {
        sensor_ui_state.set_latest_ec_voltage_mv(mv);
    }
    if let Some(mv) = voltage_readings.orp_mv {
        sensor_ui_state.set_latest_orp_voltage_mv(mv);
    }
    if let Some(c) = voltage_readings.temperature_celsius {
        sensor_ui_state.set_latest_voltage_temperature_celsius(c);
    }

    sensor_ui_state.set_latest_ph_manual_reading(manual_readings.ph_manual_reading.unwrap_or_default());
    sensor_ui_state.set_latest_ec_manual_reading(manual_readings.ec_manual_reading.unwrap_or_default());
    sensor_ui_state.set_latest_orp_manual_reading(manual_readings.orp_manual_reading.unwrap_or_default());
    sensor_ui_state.set_latest_temperature_manual_reading(manual_readings.temperature_manual_reading.unwrap_or_default());

    if crate::ui_backend::state::take_errors_dirty() {
        let errors = crate::ui_backend::state::get_recent_errors();
        let log_ui_state = ui.global::<crate::ui_types::LogUiState>();
        let entries: Vec<crate::ui_types::RecentErrorEntry> = errors.iter().rev().map(|e| {
            let category = match e.category {
                LogCategory::Sensor => "Sensor",
                LogCategory::Pump => "Pump",
                LogCategory::Dosing => "Dosing",
                LogCategory::Network => "Network",
                LogCategory::System => "System",
                LogCategory::Calibration => "Calibration",
                LogCategory::Hardware => "Hardware",
            };
            let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(e.timestamp_secs, 0)
                .unwrap_or_default();
            let timestamp = slint::format!("{}", dt.format("%m/%d %H:%M"));
            crate::ui_types::RecentErrorEntry {
                message: e.message.as_str().into(),
                category: category.into(),
                timestamp,
            }
        }).collect();
        log_ui_state.set_recent_errors(entries.as_slice().into());
    }

    if let Some(sensor_type) = crate::ui_backend::state::take_chart_request() {
        let readings = crate::ui_backend::state::get_sensor_history(sensor_type);
        let (min_acc, max_acc) = match sensor_type {
            crate::ui_types::SensorType::Ph => (
                sensor_ui_state.get_ph_min_acceptable(),
                sensor_ui_state.get_ph_max_acceptable(),
            ),
            crate::ui_types::SensorType::Conductivity => (
                sensor_ui_state.get_ec_min_acceptable(),
                sensor_ui_state.get_ec_max_acceptable(),
            ),
            crate::ui_types::SensorType::Orp => (
                sensor_ui_state.get_orp_min_acceptable(),
                sensor_ui_state.get_orp_max_acceptable(),
            ),
            crate::ui_types::SensorType::Temperature => (
                sensor_ui_state.get_temperature_min_acceptable(),
                sensor_ui_state.get_temperature_max_acceptable(),
            ),
        };
        let image = chart::render_sensor_chart(&readings, sensor_type, min_acc, max_acc, 320, 480);
        sensor_ui_state.set_chart_image(image);
        sensor_ui_state.set_chart_visible(true);
    }
}

pub async fn sync_device_config_to_ui(
    ui: &MainWindow,
    last_pumps: &mut crate::peripherals::DosingPumpStateList,
    last_outlets: &mut crate::peripherals::OutletStateList,
) {
    let config = crate::storage::get_device_config().await;

    let sensor_ui_state = ui.global::<crate::ui_types::SensorUiState>();

    sensor_ui_state.set_ph_enabled(config.ph.enabled);
    sensor_ui_state.set_ph_status(config.ph.status);
    sensor_ui_state.set_ph_min_acceptable(config.ph.min_acceptable);
    sensor_ui_state.set_ph_max_acceptable(config.ph.max_acceptable);

    sensor_ui_state.set_ec_enabled(config.ec.enabled);
    sensor_ui_state.set_ec_status(config.ec.status);
    sensor_ui_state.set_ec_min_acceptable(config.ec.min_acceptable);
    sensor_ui_state.set_ec_max_acceptable(config.ec.max_acceptable);

    sensor_ui_state.set_orp_enabled(config.orp.enabled);
    sensor_ui_state.set_orp_status(config.orp.status);
    sensor_ui_state.set_orp_min_acceptable(config.orp.min_acceptable);
    sensor_ui_state.set_orp_max_acceptable(config.orp.max_acceptable);

    sensor_ui_state.set_temperature_enabled(config.temperature.enabled);
    sensor_ui_state.set_temperature_status(config.temperature.status);
    sensor_ui_state.set_temperature_min_acceptable(config.temperature.min_acceptable);
    sensor_ui_state.set_temperature_max_acceptable(config.temperature.max_acceptable);
    sensor_ui_state.set_temperature_beta_value(config.temperature.beta_value.unwrap_or(0.0));

    if crate::ui_backend::state::take_beta_confirmed() {
        sensor_ui_state.set_temp_beta_confirm_visible(true);
    }

    let workflow_state = ui.global::<crate::ui_types::WorkflowUiState>();
    if let Some(ph_calibration) = &config.ph.calibration {
        workflow_state.set_ph_calibration_slope(ph_calibration.slope_percentage());
    }
    if let Some(ec_calibration) = &config.ec.calibration {
        let new_k = ec_calibration.cell_constant;
        if new_k != workflow_state.get_ec_cell_constant() {
            workflow_state.set_ec_cell_constant(new_k);
            if workflow_state.get_ec_calibration_state() == crate::ui_types::EcCalibrationWorkflowStep::MeasuringSolution {
                workflow_state.invoke_advance_ec_calibration();
            }
        }
    }
    if let Some(orp_calibration) = &config.orp.calibration {
        use crate::config::calibration::TimestampedValue;
        let new_timestamp = orp_calibration.get_written_timestamp().timestamp() as i32;
        if new_timestamp != workflow_state.get_orp_calibration_timestamp_secs() {
            workflow_state.set_orp_calibration_timestamp_secs(new_timestamp);
            if workflow_state.get_orp_calibration_state() == crate::ui_types::OrpCalibrationWorkflowStep::MeasuringSolution {
                workflow_state.invoke_advance_orp_calibration();
            }
        }
    }

    let pump_ui_state = ui.global::<crate::ui_types::PumpUiState>();

    cfg_if::cfg_if! {
        if #[cfg(any(test, feature = "simulation"))] {
            use std::vec::Vec;
        } else {
            use alloc::vec::Vec;
        }
    }

    if config.pumps != *last_pumps {
        let mut pump_states = Vec::new();
        for i in 0..6 {
            if let Some(pump) = crate::peripherals::DosingPump::from_int(i) {
                let pump_state = config.pumps.get_dosing_pump_state(pump);
                pump_states.push(crate::ui_types::DosingPumpUiState::from(pump_state));
            }
        }
        pump_ui_state.set_dosing_pump_states(pump_states.as_slice().into());
        *last_pumps = config.pumps.clone();
    }

    if config.outlets != *last_outlets {
        let mut outlet_states = Vec::new();
        for i in 0..4 {
            if let Some(outlet) = crate::ui_types::Outlet::from_int(i) {
                let outlet_state = config.outlets.get_outlet_state(outlet);
                outlet_states.push(crate::ui_types::OutletUiState::from(outlet_state));
            }
        }
        pump_ui_state.set_configurable_outlet_states(outlet_states.as_slice().into());
        *last_outlets = config.outlets.clone();
    }

    let app_ui_state = ui.global::<crate::ui_types::AppUiState>();
    app_ui_state.set_tank_size(config.tank_size.to_liters());
    app_ui_state.set_temperature_display_unit(config.temperature_display_unit);
    app_ui_state.set_conductivity_display_unit(config.conductivity_display_unit);

    let current_micros = embassy_time::Instant::now().as_micros();
    let current_utc_time = crate::state::get_system_time(current_micros).await;
    config.update_current_time(ui, current_utc_time);
}