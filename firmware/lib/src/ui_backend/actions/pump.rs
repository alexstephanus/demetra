use core::fmt::Debug;
use embedded_storage::Storage;
use slint::ComponentHandle;

use crate::{
    config::calibration::DoseCalibrationPoint,
    peripherals::{rtc::RealTimeClock, DosingPump, PumpController, SensorReadRaw, Pump},
    storage::update_device_config,
    ui_types::{MainWindow, PumpUiState, WorkflowUiState, UiTreatmentSolution, Status},
    units::Volume,
};

use super::MessageContext;

#[derive(Debug, Clone)]
pub struct EnableDosingPump {
    pub pump: DosingPump,
}

impl EnableDosingPump {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>().on_enable_pump(move |pump_index| {
            if let Some(pump) = DosingPump::from_int(pump_index as usize) {
                send(EnableDosingPump { pump });
            }
        });
    }

    pub async fn handle<'a, Rtc: RealTimeClock, Sensors: SensorReadRaw, Pumps: PumpController, S: Storage<Error = E>, E: Debug>(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            device_config.pumps.get_dosing_pump_state_mut(self.pump).enabled = true;
        }).await;
    }
}

#[derive(Debug, Clone)]
pub struct DisableDosingPump {
    pub pump: DosingPump,
}

impl DisableDosingPump {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>().on_disable_pump(move |pump_index| {
            if let Some(pump) = DosingPump::from_int(pump_index as usize) {
                send(DisableDosingPump { pump });
            }
        });
    }

    pub async fn handle<'a, Rtc: RealTimeClock, Sensors: SensorReadRaw, Pumps: PumpController, S: Storage<Error = E>, E: Debug>(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            device_config.pumps.get_dosing_pump_state_mut(self.pump).enabled = false;
        }).await;
    }
}

#[derive(Debug, Clone)]
pub struct SetDosingPumpStatus {
    pub pump: DosingPump,
    pub status: Status,
}

impl SetDosingPumpStatus {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>().on_set_dosing_pump_status(move |pump_index, status| {
            if let Some(pump) = DosingPump::from_int(pump_index as usize) {
                send(SetDosingPumpStatus { pump, status });
            }
        });
    }

    pub async fn handle<'a, Rtc: RealTimeClock, Sensors: SensorReadRaw, Pumps: PumpController, S: Storage<Error = E>, E: Debug>(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            device_config.pumps.get_dosing_pump_state_mut(self.pump).status = self.status;
        }).await;
    }
}

#[derive(Debug, Clone)]
pub struct RenameDosingPump {
    pub pump: DosingPump,
    pub new_name: slint::SharedString,
}

impl RenameDosingPump {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>().on_rename_pump(move |pump_index, name| {
            if let Some(pump) = DosingPump::from_int(pump_index as usize) {
                send(RenameDosingPump { pump, new_name: name });
            }
        });
    }

    pub async fn handle<'a, Rtc: RealTimeClock, Sensors: SensorReadRaw, Pumps: PumpController, S: Storage<Error = E>, E: Debug>(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            device_config.pumps.get_dosing_pump_state_mut(self.pump).name = Some(self.new_name.clone());
        }).await;
    }
}

#[derive(Debug, Clone)]
pub struct SetTreatmentSolution {
    pub pump: DosingPump,
    pub solution: UiTreatmentSolution,
}

impl SetTreatmentSolution {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>().on_update_treatment_solution(move |pump_index, solution| {
            if let Some(pump) = DosingPump::from_int(pump_index as usize) {
                send(SetTreatmentSolution { pump, solution });
            }
        });
    }

    pub async fn handle<'a, Rtc: RealTimeClock, Sensors: SensorReadRaw, Pumps: PumpController, S: Storage<Error = E>, E: Debug>(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            device_config.pumps.get_dosing_pump_state_mut(self.pump).treatment_solution = self.solution.clone();
        }).await;
    }
}

#[derive(Debug, Clone)]
pub struct CalibrateDosingPump {
    pub pump: DosingPump,
    pub first_calibration_point: DoseCalibrationPoint,
    pub second_calibration_point: DoseCalibrationPoint,
    pub third_calibration_point: DoseCalibrationPoint,
}

impl CalibrateDosingPump {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<WorkflowUiState>().on_save_dosing_pump_calibration(move |pump_number, vol_3s, vol_10s, vol_30s| {
            if let Some(pump) = DosingPump::from_int((pump_number - 1) as usize) {
                send(CalibrateDosingPump {
                    pump,
                    first_calibration_point: DoseCalibrationPoint::new(3000.0, vol_3s),
                    second_calibration_point: DoseCalibrationPoint::new(10000.0, vol_10s),
                    third_calibration_point: DoseCalibrationPoint::new(30000.0, vol_30s),
                });
            }
        });
    }

    pub async fn handle<'a, Rtc: RealTimeClock, Sensors: SensorReadRaw, Pumps: PumpController, S: Storage<Error = E>, E: Debug>(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        use crate::config::calibration::DosingPumpCalibration;

        let new_calibration = DosingPumpCalibration::new(
            self.first_calibration_point,
            self.second_calibration_point,
            self.third_calibration_point,
            ctx.current_timestamp,
        );
        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            device_config.pumps.get_dosing_pump_state_mut(self.pump).calibration = new_calibration.clone();
        }).await;
        log::info!("Updated dosing pump {:?} calibration: {:?}", self.pump, new_calibration);
    }
}

#[derive(Debug, Clone)]
pub struct RunDosingPump {
    pub pump: DosingPump,
    pub duration_seconds: u64,
}

impl RunDosingPump {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<WorkflowUiState>().on_run_dosing_pump(move |pump_number, duration_seconds| {
            if let Some(pump) = DosingPump::from_int((pump_number - 1) as usize) {
                send(RunDosingPump { pump, duration_seconds: duration_seconds as u64 });
            }
        });
    }

    pub async fn handle<'a, Rtc: RealTimeClock, Sensors: SensorReadRaw, Pumps: PumpController, S: Storage<Error = E>, E: Debug>(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        {
            let mut tc = ctx.treatment_controller.lock().await;
            if let Err(e) = tc.pump_controller.enable_pump(&Pump::Dose(self.pump)).await {
                log::error!("Failed to enable dosing pump {:?} for manual run: {:?}", self.pump, e);
                return;
            }
        }
        embassy_time::Timer::after(embassy_time::Duration::from_secs(self.duration_seconds)).await;
        {
            let mut tc = ctx.treatment_controller.lock().await;
            if let Err(e) = tc.pump_controller.disable_pump(&Pump::Dose(self.pump)).await {
                log::error!("Failed to disable dosing pump {:?} after manual run: {:?}", self.pump, e);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunDosingPumpVolumetric {
    pub pump: DosingPump,
    pub volume: Volume,
}

impl RunDosingPumpVolumetric {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<WorkflowUiState>().on_run_dosing_pump_volumetric(move |pump_number, ml| {
            if let Some(pump) = DosingPump::from_int((pump_number - 1) as usize) {
                send(RunDosingPumpVolumetric {
                    pump,
                    volume: Volume::from_milliliters(ml),
                });
            }
        });
    }

    pub async fn handle<'a, Rtc: RealTimeClock, Sensors: SensorReadRaw, Pumps: PumpController, S: Storage<Error = E>, E: Debug>(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        let config = crate::storage::get_device_config().await;
        let pump_state = config.pumps.get_dosing_pump_state(self.pump);
        let duration = pump_state.calibration.get_dose_duration(self.volume);
        log::info!("Volumetric dose: pump {:?}, {:.2}mL, {}ms", self.pump, self.volume.to_milliliters(), duration.as_millis());
        {
            let mut tc = ctx.treatment_controller.lock().await;
            if let Err(e) = tc.pump_controller.enable_pump(&Pump::Dose(self.pump)).await {
                log::error!("Failed to enable dosing pump {:?} for volumetric dose: {:?}", self.pump, e);
                return;
            }
        }
        embassy_time::Timer::after(embassy_time::Duration::from_millis(duration.as_millis() as u64)).await;
        {
            let mut tc = ctx.treatment_controller.lock().await;
            if let Err(e) = tc.pump_controller.disable_pump(&Pump::Dose(self.pump)).await {
                log::error!("Failed to disable dosing pump {:?} after volumetric dose: {:?}", self.pump, e);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct StartDosingPump {
    pub pump: DosingPump,
}

impl StartDosingPump {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<WorkflowUiState>().on_start_dosing_pump(move |pump_number| {
            if let Some(pump) = DosingPump::from_int((pump_number - 1) as usize) {
                send(StartDosingPump { pump });
            }
        });
    }

    pub async fn handle<'a, Rtc: RealTimeClock, Sensors: SensorReadRaw, Pumps: PumpController, S: Storage<Error = E>, E: Debug>(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        let mut tc = ctx.treatment_controller.lock().await;
        if let Err(e) = tc.pump_controller.enable_pump(&Pump::Dose(self.pump)).await {
            log::error!("Failed to start dosing pump {:?}: {:?}", self.pump, e);
        }
        log::info!("Started dosing pump {:?}", self.pump);
    }
}

#[derive(Debug, Clone)]
pub struct StopDosingPump {
    pub pump: DosingPump,
}

impl StopDosingPump {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<WorkflowUiState>().on_stop_dosing_pump(move |pump_number| {
            if let Some(pump) = DosingPump::from_int((pump_number - 1) as usize) {
                send(StopDosingPump { pump });
            }
        });
    }

    pub async fn handle<'a, Rtc: RealTimeClock, Sensors: SensorReadRaw, Pumps: PumpController, S: Storage<Error = E>, E: Debug>(
        self,
        ctx: &mut MessageContext<'a, '_, Rtc, Sensors, Pumps, S, E>,
    ) {
        let mut tc = ctx.treatment_controller.lock().await;
        if let Err(e) = tc.pump_controller.disable_pump(&Pump::Dose(self.pump)).await {
            log::error!("Failed to stop dosing pump {:?}: {:?}", self.pump, e);
        }
    }
}
