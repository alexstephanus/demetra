use core::sync::atomic::{AtomicU8, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embedded_storage::Storage;

pub const RESERVOIR_STATE_IDLE: u8 = 0;
pub const RESERVOIR_STATE_TREATMENT: u8 = 1;
pub const RESERVOIR_STATE_FILL: u8 = 2;

pub static RESERVOIR_OPERATION_STATE: AtomicU8 = AtomicU8::new(RESERVOIR_STATE_IDLE);

use crate::logging::{flash_log_error, flash_log_sensor_readings, LoggableError};
use crate::{
    config::{
        calibration::{OrpMeasurementPoint, PhMeasurementPoint},
        device_config::DeviceConfig,
        outlet_schedule::compute_next_schedule_change,
    },
    peripherals::{dosing, Pump, PumpController, SensorReadRaw, CURRENT_CUTOFF, DosingPump},
    storage::get_device_config,
    storage::ring_buffer::EmptyMetadata,
    ui_types::{Outlet, MainWindow, OutletMode},
    units::{Conductivity, Temperature},
};

pub struct SensorReadings {
    pub temperature: Option<Temperature>,
    pub ec: Option<Conductivity>,
    pub ph: Option<PhMeasurementPoint>,
    pub orp: Option<OrpMeasurementPoint>,
}

pub static SCHEDULE_CHANGE_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub async fn process_ui_messages<
    'a,
    Rtc: crate::peripherals::rtc::RealTimeClock,
    Sensors: SensorReadRaw,
    Pumps: crate::peripherals::PumpController,
    S: Storage<Error = E>,
    E: core::fmt::Debug,
>(
    mut rtc: Rtc,
    treatment_controller: &'a crate::peripherals::TreatmentControllerMutex<'a, Sensors, Pumps>,
    mut config_buffer: crate::storage::ring_buffer::RingBuffer<
        crate::config::device_config::DeviceConfig,
        EmptyMetadata,
        S,
        E,
    >,
    ticks_fn: impl Fn() -> u64,
) {
    use crate::ui_backend::actions::{UI_ACTION_CHANNEL, MessageContext};

    let ui_message_receiver = UI_ACTION_CHANNEL.receiver();
    loop {
        let message = ui_message_receiver.receive().await;
        log::debug!("UI message received: {:?}", message);
        let current_ticks = ticks_fn();
        let current_timestamp = crate::state::get_system_time(current_ticks).await;
        let mut ctx = MessageContext {
            current_timestamp,
            current_ticks,
            rtc: &mut rtc,
            treatment_controller,
            config_buffer: &mut config_buffer,
        };
        crate::ui_backend::actions::dispatch(message, &mut ctx).await;
    };
}

pub async fn update_clock_task(
    ui: &MainWindow,
    ticks_fn: impl Fn() -> u64,
) {
    let mut ticker = embassy_time::Ticker::every(embassy_time::Duration::from_secs(1));
    loop {
        ticker.next().await;

        let current_utc_time = crate::state::get_system_time(ticks_fn()).await;
        let device_config = get_device_config().await;
        device_config.update_current_time(ui, current_utc_time);
    }
}

const SCHEDULABLE_OUTLETS: [Outlet; 4] = [
    Outlet::One,
    Outlet::Two,
    Outlet::Three,
    Outlet::Four,
];

fn is_schedulable(mode: &OutletMode) -> bool {
    matches!(mode, OutletMode::FertigationPump | OutletMode::GeneralPurpose)
}

pub async fn outlet_scheduler_task<
    'a,
    Sensors: SensorReadRaw,
    Pumps: PumpController,
>(
    treatment_controller: &crate::peripherals::TreatmentControllerMutex<'a, Sensors, Pumps>,
    ticks_fn: impl Fn() -> u64,
) {
    log::info!("Outlet scheduler task started");

    loop {
        let config = get_device_config().await;
        let now = crate::state::get_system_time(ticks_fn()).await;

        let empty_schedule = crate::config::outlet_schedule::OutletSchedule::new();
        let outlet_states: [crate::peripherals::OutletState; 4] = core::array::from_fn(|i| {
            config.outlets.get_outlet_state(SCHEDULABLE_OUTLETS[i])
        });
        let schedule_refs: [&crate::config::outlet_schedule::OutletSchedule; 4] = core::array::from_fn(|i| {
            let state = &outlet_states[i];
            if state.enabled && is_schedulable(&state.mode) {
                &state.schedule
            } else {
                &empty_schedule
            }
        });

        match compute_next_schedule_change(now, &schedule_refs, config.time_display_config.timezone) {
            Some(transitions) => {
                let sleep_duration = transitions.at - now;
                let sleep_secs = sleep_duration.num_seconds().max(1) as u64;
                log::info!(
                    "Next schedule transition at {} (in {}s, {} outlet(s))",
                    transitions.at, sleep_secs, transitions.outlets.len()
                );

                let sleep_future = embassy_time::Timer::after(
                    embassy_time::Duration::from_secs(sleep_secs),
                );
                let signal_future = SCHEDULE_CHANGE_SIGNAL.wait();

                match embassy_futures::select::select(sleep_future, signal_future).await {
                    embassy_futures::select::Either::First(_) => {
                        let mut tc = treatment_controller.lock().await;
                        for (outlet_idx, turn_on) in &transitions.outlets {
                            let outlet = SCHEDULABLE_OUTLETS[*outlet_idx];
                            let pump = Pump::Cfg(outlet);
                            if *turn_on {
                                if let Err(e) = tc.pump_controller.enable_pump(&pump).await {
                                    log::error!("Failed to enable outlet {:?} on schedule: {:?}", outlet, e);
                                }
                            } else {
                                if let Err(e) = tc.pump_controller.disable_pump(&pump).await {
                                    log::error!("Failed to disable outlet {:?} on schedule: {:?}", outlet, e);
                                }
                            }
                        }
                    }
                    embassy_futures::select::Either::Second(_) => {
                        log::info!("Schedule changed, recomputing");
                    }
                }
            }
            None => {
                let signal_future = SCHEDULE_CHANGE_SIGNAL.wait();
                let sleep_future = embassy_time::Timer::after(
                    embassy_time::Duration::from_secs(3600),
                );
                embassy_futures::select::select(sleep_future, signal_future).await;
            }
        }
    }
}

async fn read_sensors<Sensors: SensorReadRaw>(
    sensor_controller: &mut crate::peripherals::SensorController<'_, Sensors>,
    config: &DeviceConfig,
) -> SensorReadings {
    let temperature = match (config.temperature.enabled, &config.temperature.beta_value) {
        (true, Some(beta)) => {
            match sensor_controller.measure_temperature(*beta).await {
                Ok(temp) => Some(temp),
                Err(e) => {
                    flash_log_error(&LoggableError::from(e));
                    None
                }
            }
        }
        _ => None,
    };

    let temp_for_compensation = temperature.unwrap_or_default();

    let ec = if config.ec.enabled {
        if let Some(ec_cal) = config.ec.calibration.as_ref() {
            match sensor_controller.measure_conductivity(ec_cal, temp_for_compensation).await {
                Ok(m) => Some(m),
                Err(e) => {
                    flash_log_error(&LoggableError::from(e));
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    let ph = if config.ph.enabled {
        if let Some(ph_cal) = config.ph.calibration.as_ref() {
            match sensor_controller.measure_ph(ph_cal, temp_for_compensation).await {
                Ok(m) => Some(m),
                Err(e) => {
                    flash_log_error(&LoggableError::from(e));
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    let orp = if config.orp.enabled {
        if let Some(orp_cal) = config.orp.calibration.as_ref() {
            match sensor_controller.measure_orp(orp_cal).await {
                Ok(m) => Some(m),
                Err(e) => {
                    flash_log_error(&LoggableError::from(e));
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    SensorReadings { temperature, ec, ph, orp }
}

async fn publish_sensor_readings(readings: &SensorReadings) {
    crate::ui_backend::state::update_current_sensor_readings(readings).await;
    flash_log_sensor_readings(readings);
}

async fn apply_treatment<Sensors: SensorReadRaw, Pumps: PumpController>(
    tc: &crate::peripherals::TreatmentControllerMutex<'_, Sensors, Pumps>,
    config: &DeviceConfig,
    readings: &SensorReadings,
) {
    use dosing::PrioritizedTreatment;

    let dose_result = match dosing::select_dosing_action(config, readings) {
        PrioritizedTreatment::None => Ok(()),
        PrioritizedTreatment::RaiseConductivity(ec) => {
            dosing::dose_ec_step(tc, config, ec).await
        }
        PrioritizedTreatment::RaisePh(ph) => {
            dosing::dose_ph_step(tc, config, &ph, true).await
        }
        PrioritizedTreatment::LowerPh(ph) => {
            dosing::dose_ph_step(tc, config, &ph, false).await
        }
        PrioritizedTreatment::RaiseOrp(orp) => {
            dosing::dose_orp_step(tc, config, &orp).await
        }
    };
    if let Err(e) = dose_result {
        flash_log_error(&e);
    }
}

pub async fn run_dosing_cycle<
    'a,
    Sensors: SensorReadRaw,
    Pumps: PumpController,
>(
    treatment_controller: &crate::peripherals::TreatmentControllerMutex<'a, Sensors, Pumps>,
) {
    if RESERVOIR_OPERATION_STATE.compare_exchange(
        RESERVOIR_STATE_IDLE,
        RESERVOIR_STATE_TREATMENT,
        Ordering::AcqRel,
        Ordering::Acquire,
    ).is_err() {
        log::info!("Skipping dosing cycle - reservoir operation in progress");
        return;
    }

    let config = get_device_config().await;

    let readings = {
        let mut tc = treatment_controller.lock().await;
        let readings = read_sensors(&mut tc.sensor_controller, &config).await;
        if let Err(e) = tc.sensor_controller.turn_sensors_off().await {
            flash_log_error(&LoggableError::from(e));
        }
        readings
    };

    publish_sensor_readings(&readings).await;

    apply_treatment(treatment_controller, &config, &readings).await;

    RESERVOIR_OPERATION_STATE.store(RESERVOIR_STATE_IDLE, Ordering::Release);
}

pub async fn run_fill_cycle<
    'a,
    Sensors: SensorReadRaw,
    Pumps: PumpController,
    FloatPin: embedded_hal_async::digital::Wait,
>(
    float_pin: &mut FloatPin,
    treatment_controller: &crate::peripherals::TreatmentControllerMutex<'a, Sensors, Pumps>,
) {
    loop {
        if let Err(e) = float_pin.wait_for_rising_edge().await {
            log::error!("Float sensor GPIO error: {:?}", e);
            continue;
        }
        log::info!("Float sensor triggered.  Starting reservoir top-up."); 
        perform_fill(treatment_controller).await;
    }
}

async fn perform_fill<
    'a,
    Sensors: SensorReadRaw,
    Pumps: PumpController,
>(
    treatment_controller: &crate::peripherals::TreatmentControllerMutex<'a, Sensors, Pumps>,
) {
    loop {
        match RESERVOIR_OPERATION_STATE.compare_exchange(
            RESERVOIR_STATE_IDLE,
            RESERVOIR_STATE_FILL,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => break,
            Err(state) => {
                log::info!("Waiting for reservoir operation to complete before filling (state: {})", state);
                embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
            }
        }
    }

    if let Err(e) = try_perform_fill(treatment_controller).await {
        flash_log_error(&e);
    }

    RESERVOIR_OPERATION_STATE.store(RESERVOIR_STATE_IDLE, Ordering::Release);
}

async fn try_perform_fill<
    'a,
    Sensors: SensorReadRaw,
    Pumps: PumpController,
>(
    treatment_controller: &crate::peripherals::TreatmentControllerMutex<'a, Sensors, Pumps>,
) -> Result<(), LoggableError> {
    let config = get_device_config().await;

    let Some(solenoid_state) = config.outlets.get_solenoid_outlet().cloned() else {
        log::warn!("Float sensor triggered but no solenoid outlet configured");
        return Ok(());
    };

    let Some(fill_secs) = solenoid_state.max_fill_seconds.map(|s| s as u64) else {
        log::warn!("Solenoid outlet has no fill duration configured, skipping fill");
        return Ok(());
    };

    let solenoid_pump = Pump::Cfg(solenoid_state.outlet);

    {
        let mut tc = treatment_controller.lock().await;
        tc.pump_controller.enable_pump(&solenoid_pump).await?;
    }

    log::info!("Solenoid enabled for {}s fill", fill_secs);
    embassy_time::Timer::after(embassy_time::Duration::from_secs(fill_secs)).await;

    {
        let mut tc = treatment_controller.lock().await;
        tc.pump_controller.disable_pump(&solenoid_pump).await?;
    }

    if let Some(stir_outlet) = config.outlets.get_stir_outlet() {
        let stir_secs = match stir_outlet.stir_seconds {
            Some(s) => s as u64,
            None => {
                log::warn!("Stir outlet has no duration configured, defaulting to 10s");
                10
            }
        };
        let stir_pump = Pump::Cfg(stir_outlet.outlet);
        {
            let mut tc = treatment_controller.lock().await;
            tc.pump_controller.enable_pump(&stir_pump).await?;
        }
        embassy_time::Timer::after(embassy_time::Duration::from_secs(stir_secs)).await;
        {
            let mut tc = treatment_controller.lock().await;
            tc.pump_controller.disable_pump(&stir_pump).await?;
        }
    }

    Ok(())
}

const ALL_DOSING_PUMPS: [DosingPump; 6] = [
    DosingPump::DoseOne,
    DosingPump::DoseTwo,
    DosingPump::DoseThree,
    DosingPump::DoseFour,
    DosingPump::DoseFive,
    DosingPump::DoseSix,
];

const ALL_OUTLETS: [Outlet; 4] = [
    Outlet::One,
    Outlet::Two,
    Outlet::Three,
    Outlet::Four,
];

pub async fn check_pump_currents<
    Sensors: SensorReadRaw,
    Pumps: PumpController,
>(
    treatment_controller: &crate::peripherals::TreatmentControllerMutex<'_, Sensors, Pumps>,
) {
    let mut periphs = treatment_controller.lock().await;

    let any_dosing_enabled = ALL_DOSING_PUMPS.iter().any(|dp| {
        periphs.pump_controller.is_pump_enabled(&Pump::Dose(*dp)).unwrap_or(false)
    });
    match periphs.pump_controller.read_current(&Pump::Dose(DosingPump::DoseOne)).await {
        Ok(current) => {
            let has_current = current >= CURRENT_CUTOFF;
            if any_dosing_enabled && !has_current {
                flash_log_error(&LoggableError::Pump(crate::peripherals::PumpError::NoCurrent));
            } else if !any_dosing_enabled && has_current {
                flash_log_error(&LoggableError::Pump(crate::peripherals::PumpError::UnexpectedCurrent));
                periphs.pump_controller.kill_relay();
            }
        }
        Err(e) => {
            flash_log_error(&LoggableError::Pump(e));
        }
    }

    for outlet in &ALL_OUTLETS {
        let pump = Pump::Cfg(*outlet);
        let enabled = match periphs.pump_controller.is_pump_enabled(&pump) {
            Ok(e) => e,
            Err(e) => {
                flash_log_error(&LoggableError::Pump(e));
                continue;
            }
        };
        match periphs.pump_controller.read_current(&pump).await {
            Ok(current) => {
                let has_current = current >= CURRENT_CUTOFF;
                if enabled && !has_current {
                    flash_log_error(&LoggableError::Pump(crate::peripherals::PumpError::NoCurrent));
                } else if !enabled && has_current {
                    flash_log_error(&LoggableError::Pump(crate::peripherals::PumpError::UnexpectedCurrent));
                    periphs.pump_controller.kill_relay();
                }
            }
            Err(e) => {
                flash_log_error(&LoggableError::Pump(e));
            }
        }
    }
}
