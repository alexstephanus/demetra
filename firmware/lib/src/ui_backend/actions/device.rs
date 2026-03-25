use core::fmt::Debug;
use embedded_storage::Storage;
use slint::ComponentHandle;

use crate::{
    peripherals::{rtc::RealTimeClock, PumpController, SensorReadRaw},
    storage::update_device_config,
    ui_types::{AppUiState, ConductivityDisplayUnit, MainWindow, TemperatureDisplayUnit},
    units::Volume,
};

use super::MessageContext;

#[derive(Debug, Clone)]
pub struct SetTankSize {
    pub tank_size: Volume,
}

impl SetTankSize {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<AppUiState>()
            .on_set_tank_size(move |tank_size_liters| {
                send(SetTankSize {
                    tank_size: Volume::from_liters(tank_size_liters),
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
        update_device_config(ctx.config_buffer, ctx.current_timestamp, |device_config| {
            device_config.tank_size = self.tank_size;
        })
        .await;
    }
}

#[derive(Debug, Clone)]
pub struct SetTemperatureDisplayUnit {
    pub display_unit: TemperatureDisplayUnit,
}

impl SetTemperatureDisplayUnit {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<AppUiState>()
            .on_set_temperature_display_unit(move |display_unit| {
                send(SetTemperatureDisplayUnit { display_unit });
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
            device_config.temperature_display_unit = self.display_unit;
        })
        .await;
    }
}

#[derive(Debug, Clone)]
pub struct SetConductivityDisplayUnit {
    pub display_unit: ConductivityDisplayUnit,
}

impl SetConductivityDisplayUnit {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<AppUiState>()
            .on_set_conductivity_display_unit(move |display_unit| {
                send(SetConductivityDisplayUnit { display_unit });
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
            device_config.conductivity_display_unit = self.display_unit;
        })
        .await;
    }
}

#[derive(Debug, Clone)]
pub struct SetDate {
    pub year: i32,
    pub month: i32,
    pub day: i32,
}

impl SetDate {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<AppUiState>()
            .on_set_date(move |year, month, day| {
                send(SetDate { year, month, day });
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
        let old_info = crate::state::read_system_time_info().await;
        let new_info =
            match old_info.update_date(self.year, self.month, self.day, ctx.current_ticks) {
                Ok(info) => info,
                Err(e) => {
                    log::error!("Failed to update system date: {}", e);
                    return;
                }
            };
        let new_datetime = new_info.get_current_time(ctx.current_ticks);
        if let Err(e) = ctx.rtc.set_datetime(new_datetime).await {
            log::error!("Failed to set RTC datetime: {}", e);
            return;
        }
        crate::state::set_system_time_info(new_info).await;
    }
}

#[derive(Debug, Clone)]
pub struct SetTime {
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
}

impl SetTime {
    pub fn register_callback(ui: &MainWindow, send: impl Fn(Self) + 'static + Clone) {
        ui.global::<AppUiState>()
            .on_set_time(move |hour, minute, second| {
                send(SetTime {
                    hour,
                    minute,
                    second,
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
        let old_info = crate::state::read_system_time_info().await;
        let new_info =
            match old_info.update_time(self.hour, self.minute, self.second, ctx.current_ticks) {
                Ok(info) => info,
                Err(e) => {
                    log::error!("Failed to update system time: {}", e);
                    return;
                }
            };
        let new_datetime = new_info.get_current_time(ctx.current_ticks);
        if let Err(e) = ctx.rtc.set_datetime(new_datetime).await {
            log::error!("Failed to set RTC datetime: {}", e);
            return;
        }
        crate::state::set_system_time_info(new_info).await;
    }
}
