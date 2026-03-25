use core::fmt::Debug;
use embedded_storage::Storage;
use slint::ComponentHandle;

use crate::{
    peripherals::{rtc::RealTimeClock, Pump, PumpController, SensorReadRaw},
    storage::update_device_config,
    ui_types::{MainWindow, Outlet, OutletMode, PumpUiState, UiScheduledRunWindow},
};

use super::MessageContext;

#[derive(Debug, Clone)]
pub struct EnableOutlet {
    pub outlet: Outlet,
}

impl EnableOutlet {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>()
            .on_enable_outlet(move |outlet_index| {
                if let Some(outlet) = Outlet::from_int(outlet_index as usize) {
                    send(EnableOutlet { outlet });
                }
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
            device_config
                .outlets
                .get_outlet_state_mut(self.outlet)
                .enabled = true;
        })
        .await;
        crate::tasks::SCHEDULE_CHANGE_SIGNAL.signal(());
    }
}

#[derive(Debug, Clone)]
pub struct DisableOutlet {
    pub outlet: Outlet,
}

impl DisableOutlet {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>()
            .on_disable_outlet(move |outlet_index| {
                if let Some(outlet) = Outlet::from_int(outlet_index as usize) {
                    send(DisableOutlet { outlet });
                }
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
        {
            let mut tc = ctx.treatment_controller.lock().await;
            if let Err(e) = tc
                .pump_controller
                .disable_pump(&Pump::Cfg(self.outlet))
                .await
            {
                log::error!("Failed to disable outlet {:?}: {:?}", self.outlet, e);
            }
        }
        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            device_config
                .outlets
                .get_outlet_state_mut(self.outlet)
                .enabled = false;
        })
        .await;
        crate::tasks::SCHEDULE_CHANGE_SIGNAL.signal(());
    }
}

#[derive(Debug, Clone)]
pub struct RenameOutlet {
    pub outlet: Outlet,
    pub new_name: slint::SharedString,
}

impl RenameOutlet {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>()
            .on_rename_outlet(move |outlet_index, name| {
                if let Some(outlet) = Outlet::from_int(outlet_index as usize) {
                    send(RenameOutlet {
                        outlet,
                        new_name: name,
                    });
                }
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
            device_config.outlets.get_outlet_state_mut(self.outlet).name =
                Some(self.new_name.clone());
        })
        .await;
    }
}

#[derive(Debug, Clone)]
pub struct SetOutletMode {
    pub outlet: Outlet,
    pub outlet_mode: OutletMode,
}

impl SetOutletMode {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>()
            .on_change_outlet_mode(move |outlet_index, outlet_mode| {
                if let Some(outlet) = Outlet::from_int(outlet_index as usize) {
                    send(SetOutletMode {
                        outlet,
                        outlet_mode,
                    });
                }
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
        {
            let mut tc = ctx.treatment_controller.lock().await;
            if let Err(e) = tc
                .pump_controller
                .disable_pump(&Pump::Cfg(self.outlet))
                .await
            {
                log::error!("Failed to disable outlet {:?}: {:?}", self.outlet, e);
            }
        }
        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            device_config.outlets.get_outlet_state_mut(self.outlet).mode = self.outlet_mode;
        })
        .await;
        crate::tasks::SCHEDULE_CHANGE_SIGNAL.signal(());
    }
}

#[derive(Debug, Clone)]
pub struct RunOutlet {
    pub outlet: Outlet,
    pub duration_seconds: u64,
}

impl RunOutlet {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>()
            .on_run_outlet_manually(move |outlet_index, duration_seconds| {
                if let Some(outlet) = Outlet::from_int(outlet_index as usize) {
                    send(RunOutlet {
                        outlet,
                        duration_seconds: duration_seconds as u64,
                    });
                }
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
        {
            let mut tc = ctx.treatment_controller.lock().await;
            if let Err(e) = tc
                .pump_controller
                .enable_pump(&Pump::Cfg(self.outlet))
                .await
            {
                log::error!(
                    "Failed to enable outlet {:?} for manual run: {:?}",
                    self.outlet,
                    e
                );
                return;
            }
        }
        embassy_time::Timer::after(embassy_time::Duration::from_secs(self.duration_seconds)).await;
        {
            let mut tc = ctx.treatment_controller.lock().await;
            if let Err(e) = tc
                .pump_controller
                .disable_pump(&Pump::Cfg(self.outlet))
                .await
            {
                log::error!(
                    "Failed to disable outlet {:?} after manual run: {:?}",
                    self.outlet,
                    e
                );
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AddScheduleWindow {
    pub outlet: Outlet,
    pub window: UiScheduledRunWindow,
}

impl AddScheduleWindow {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>()
            .on_schedule_add_window(move |outlet_index, window| {
                if let Some(outlet) = Outlet::from_int(outlet_index as usize) {
                    send(AddScheduleWindow { outlet, window });
                }
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
        use crate::config::outlet_schedule::ScheduledEvent;

        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            let outlet_config = device_config.outlets.get_outlet_state_mut(self.outlet);
            match ScheduledEvent::try_from(self.window.clone()) {
                Ok(event) => {
                    outlet_config.schedule.add_event(event);
                }
                Err(_) => {
                    log::error!(
                        "Invalid schedule window from UI for outlet {:?}, discarding",
                        self.outlet
                    );
                }
            }
        })
        .await;
        crate::tasks::SCHEDULE_CHANGE_SIGNAL.signal(());
    }
}

#[derive(Debug, Clone)]
pub struct UpdateScheduleWindow {
    pub outlet: Outlet,
    pub index: i32,
    pub window: UiScheduledRunWindow,
}

impl UpdateScheduleWindow {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>()
            .on_schedule_update_window(move |outlet_index, index, window| {
                if let Some(outlet) = Outlet::from_int(outlet_index as usize) {
                    send(UpdateScheduleWindow {
                        outlet,
                        index,
                        window,
                    });
                }
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
        use crate::config::outlet_schedule::ScheduledEvent;

        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            let outlet_config = device_config.outlets.get_outlet_state_mut(self.outlet);
            match ScheduledEvent::try_from(self.window.clone()) {
                Ok(event) => {
                    outlet_config.schedule.remove_event(self.index as usize);
                    outlet_config.schedule.add_event(event);
                }
                Err(_) => {
                    log::error!(
                        "Invalid updated schedule window from UI for outlet {:?}, discarding",
                        self.outlet
                    );
                }
            }
        })
        .await;
        crate::tasks::SCHEDULE_CHANGE_SIGNAL.signal(());
    }
}

#[derive(Debug, Clone)]
pub struct DeleteScheduleWindow {
    pub outlet: Outlet,
    pub index: i32,
}

impl DeleteScheduleWindow {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>()
            .on_schedule_delete_window(move |outlet_index, index| {
                if let Some(outlet) = Outlet::from_int(outlet_index as usize) {
                    send(DeleteScheduleWindow { outlet, index });
                }
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
            device_config
                .outlets
                .get_outlet_state_mut(self.outlet)
                .schedule
                .remove_event(self.index as usize);
        })
        .await;
        crate::tasks::SCHEDULE_CHANGE_SIGNAL.signal(());
    }
}

#[derive(Debug, Clone)]
pub struct SetSolenoidFillTime {
    pub outlet: Outlet,
    pub seconds: u32,
}

impl SetSolenoidFillTime {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>()
            .on_update_solenoid_fill_time(move |outlet_index, seconds| {
                if let Some(outlet) = Outlet::from_int(outlet_index as usize) {
                    send(SetSolenoidFillTime {
                        outlet,
                        seconds: seconds as u32,
                    });
                }
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
            device_config
                .outlets
                .get_outlet_state_mut(self.outlet)
                .max_fill_seconds = Some(self.seconds);
        })
        .await;
    }
}

#[derive(Debug, Clone)]
pub struct SetStirPumpDuration {
    pub outlet: Outlet,
    pub seconds: u32,
}

impl SetStirPumpDuration {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<PumpUiState>()
            .on_update_stir_duration(move |outlet_index, seconds| {
                if let Some(outlet) = Outlet::from_int(outlet_index as usize) {
                    send(SetStirPumpDuration {
                        outlet,
                        seconds: seconds as u32,
                    });
                }
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
            device_config
                .outlets
                .get_outlet_state_mut(self.outlet)
                .stir_seconds = Some(self.seconds);
        })
        .await;
    }
}
