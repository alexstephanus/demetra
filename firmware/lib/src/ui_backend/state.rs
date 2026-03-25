use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    blocking_mutex::Mutex as BlockingMutex,
    rwlock::RwLock,
};
use slint::SharedString;

cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::vec::Vec;
        use std::string::String;
    } else {
        use alloc::vec::Vec;
        use alloc::string::String;
    }
}

use crate::logging::LogCategory;
use crate::ui_types::{Pump, SensorType};

type LockedState<T> = RwLock<CriticalSectionRawMutex, T>;

#[derive(Default, Clone, Copy)]
pub struct CurrentSensorReadings {
    pub ph_value: Option<f32>,
    pub ec_value: Option<f32>,
    pub orp_value: Option<f32>,
    pub temperature_celsius: Option<f32>,
}

#[derive(Default, Clone, Copy)]
pub struct VoltageReadings {
    pub ph_mv: Option<f32>,
    pub ec_mv: Option<f32>,
    pub orp_mv: Option<f32>,
    pub temperature_celsius: Option<f32>,
}

#[derive(Default, Clone)]
pub struct ManualSensorReadings {
    pub ph_manual_reading: Option<SharedString>,
    pub ec_manual_reading: Option<SharedString>,
    pub orp_manual_reading: Option<SharedString>,
    pub temperature_manual_reading: Option<SharedString>,
}

static CURRENT_SENSOR_READINGS: LockedState<CurrentSensorReadings> = RwLock::new(CurrentSensorReadings {
    ph_value: None,
    ec_value: None,
    orp_value: None,
    temperature_celsius: None,
});

static VOLTAGE_READINGS: LockedState<VoltageReadings> = RwLock::new(VoltageReadings {
    ph_mv: None,
    ec_mv: None,
    orp_mv: None,
    temperature_celsius: None,
});

#[derive(Clone, Copy, Default)]
pub struct PumpRuntimeState {
    pub dosing_pumps: [bool; 6],
    pub outlets: [bool; 4],
}


static PUMP_RUNTIME_STATE: LockedState<PumpRuntimeState> = RwLock::new(PumpRuntimeState {
    dosing_pumps: [false; 6],
    outlets: [false; 4],
});

static MANUAL_SENSOR_READINGS: LockedState<ManualSensorReadings> = RwLock::new(ManualSensorReadings {
    ph_manual_reading: None,
    ec_manual_reading: None,
    orp_manual_reading: None,
    temperature_manual_reading: None,
});

pub async fn get_pump_runtime_state() -> PumpRuntimeState {
    *PUMP_RUNTIME_STATE.read().await
}

pub async fn set_pump_active(pump: &Pump, active: bool) {
    let mut state = PUMP_RUNTIME_STATE.write().await;
    match pump {
        Pump::Dose(d) => state.dosing_pumps[d.to_int()] = active,
        Pump::Cfg(o) => state.outlets[o.to_int()] = active,
    }
}

pub async fn clear_all_pump_states() {
    *PUMP_RUNTIME_STATE.write().await = PumpRuntimeState::default();
}

pub async fn get_current_sensor_readings() -> CurrentSensorReadings {
    *CURRENT_SENSOR_READINGS.read().await
}

pub async fn get_voltage_readings() -> VoltageReadings {
    *VOLTAGE_READINGS.read().await
}

pub async fn get_manual_sensor_readings() -> ManualSensorReadings {
    (*MANUAL_SENSOR_READINGS.read().await).clone()
}

pub async fn update_current_sensor_value(sensor_type: SensorType, value: f32) {
    let mut readings = CURRENT_SENSOR_READINGS.write().await;
    match sensor_type {
        SensorType::Ph => readings.ph_value = Some(value),
        SensorType::Conductivity => readings.ec_value = Some(value),
        SensorType::Orp => readings.orp_value = Some(value),
        SensorType::Temperature => readings.temperature_celsius = Some(value),
    }
}

pub async fn update_current_sensor_readings(sensor_readings: &crate::tasks::SensorReadings) {
    let mut readings = CURRENT_SENSOR_READINGS.write().await;
    if let Some(t) = &sensor_readings.temperature {
        readings.temperature_celsius = Some(t.celsius());
    }
    if let Some(c) = &sensor_readings.ec {
        readings.ec_value = Some(c.us_per_cm());
    }
    if let Some(p) = &sensor_readings.ph {
        readings.ph_value = Some(p.ph_value);
    }
    if let Some(o) = &sensor_readings.orp {
        readings.orp_value = Some(o.voltage.mv());
    }
}

pub async fn update_voltage_reading(sensor_type: SensorType, value: f32) {
    let mut readings = VOLTAGE_READINGS.write().await;
    match sensor_type {
        SensorType::Ph => readings.ph_mv = Some(value),
        SensorType::Conductivity => readings.ec_mv = Some(value),
        SensorType::Orp => readings.orp_mv = Some(value),
        SensorType::Temperature => readings.temperature_celsius = Some(value),
    }
}

pub async fn update_manual_sensor_reading(sensor_type: SensorType, reading: SharedString) {
    let mut readings = MANUAL_SENSOR_READINGS.write().await;
    match sensor_type {
        SensorType::Ph => readings.ph_manual_reading = Some(reading),
        SensorType::Conductivity => readings.ec_manual_reading = Some(reading),
        SensorType::Orp => readings.orp_manual_reading = Some(reading),
        SensorType::Temperature => readings.temperature_manual_reading = Some(reading),
    }
}

pub async fn clear_voltage_reading(sensor_type: SensorType) {
    let mut readings = VOLTAGE_READINGS.write().await;
    match sensor_type {
        SensorType::Ph => readings.ph_mv = None,
        SensorType::Conductivity => readings.ec_mv = None,
        SensorType::Orp => readings.orp_mv = None,
        SensorType::Temperature => readings.temperature_celsius = None,
    }
}

pub async fn clear_manual_sensor_reading(sensor_type: SensorType) {
    let mut readings = MANUAL_SENSOR_READINGS.write().await;
    match sensor_type {
        SensorType::Ph => readings.ph_manual_reading = None,
        SensorType::Conductivity => readings.ec_manual_reading = None,
        SensorType::Orp => readings.orp_manual_reading = None,
        SensorType::Temperature => readings.temperature_manual_reading = None,
    }
}

const MAX_SENSOR_HISTORY_ENTRIES: usize = 1000;

#[derive(Clone)]
pub struct SensorHistory {
    pub ph: Vec<(i64, f32)>,
    pub ec: Vec<(i64, f32)>,
    pub orp: Vec<(i64, f32)>,
    pub temperature: Vec<(i64, f32)>,
}

impl SensorHistory {
    fn get_mut(&mut self, sensor_type: SensorType) -> &mut Vec<(i64, f32)> {
        match sensor_type {
            SensorType::Ph => &mut self.ph,
            SensorType::Conductivity => &mut self.ec,
            SensorType::Orp => &mut self.orp,
            SensorType::Temperature => &mut self.temperature,
        }
    }

    fn get(&self, sensor_type: SensorType) -> &Vec<(i64, f32)> {
        match sensor_type {
            SensorType::Ph => &self.ph,
            SensorType::Conductivity => &self.ec,
            SensorType::Orp => &self.orp,
            SensorType::Temperature => &self.temperature,
        }
    }
}

static SENSOR_HISTORY: BlockingMutex<CriticalSectionRawMutex, RefCell<SensorHistory>> =
    BlockingMutex::new(RefCell::new(SensorHistory {
        ph: Vec::new(),
        ec: Vec::new(),
        orp: Vec::new(),
        temperature: Vec::new(),
    }));

pub fn push_sensor_reading(sensor_type: SensorType, timestamp_secs: i64, value: f32) {
    SENSOR_HISTORY.lock(|cell| {
        let mut history = cell.borrow_mut();
        let vec = history.get_mut(sensor_type);
        if vec.len() >= MAX_SENSOR_HISTORY_ENTRIES {
            vec.remove(0);
        }
        vec.push((timestamp_secs, value));
    });
}

pub fn push_sensor_readings_bulk(sensor_type: SensorType, readings: &[(i64, f32)]) {
    SENSOR_HISTORY.lock(|cell| {
        let mut history = cell.borrow_mut();
        let vec = history.get_mut(sensor_type);
        for &(ts, val) in readings {
            if vec.len() >= MAX_SENSOR_HISTORY_ENTRIES {
                vec.remove(0);
            }
            vec.push((ts, val));
        }
    });
}

pub fn get_sensor_history(sensor_type: SensorType) -> Vec<(i64, f32)> {
    SENSOR_HISTORY.lock(|cell| {
        let history = cell.borrow();
        history.get(sensor_type).clone()
    })
}

const MAX_RECENT_ERRORS: usize = 50;

#[derive(Clone)]
pub struct RecentError {
    pub message: String,
    pub category: LogCategory,
    pub timestamp_secs: i64,
}

static RECENT_ERRORS: BlockingMutex<CriticalSectionRawMutex, RefCell<Vec<RecentError>>> =
    BlockingMutex::new(RefCell::new(Vec::new()));

static ERRORS_DIRTY: AtomicBool = AtomicBool::new(false);

pub fn push_recent_error(error: RecentError) {
    RECENT_ERRORS.lock(|cell| {
        let mut errors = cell.borrow_mut();
        if errors.len() >= MAX_RECENT_ERRORS {
            errors.remove(0);
        }
        errors.push(error);
    });
    ERRORS_DIRTY.store(true, Ordering::Release);
}

pub fn take_errors_dirty() -> bool {
    ERRORS_DIRTY.swap(false, Ordering::AcqRel)
}

pub fn get_recent_errors() -> Vec<RecentError> {
    RECENT_ERRORS.lock(|cell| cell.borrow().clone())
}

static BETA_CONFIRMED: AtomicBool = AtomicBool::new(false);

pub fn set_beta_confirmed() {
    BETA_CONFIRMED.store(true, Ordering::Release);
}

pub fn take_beta_confirmed() -> bool {
    BETA_CONFIRMED.swap(false, Ordering::AcqRel)
}

const CHART_REQUEST_NONE: u8 = 0;
const CHART_REQUEST_PH: u8 = 1;
const CHART_REQUEST_EC: u8 = 2;
const CHART_REQUEST_ORP: u8 = 3;
const CHART_REQUEST_TEMPERATURE: u8 = 4;

static CHART_REQUESTED: AtomicU8 = AtomicU8::new(CHART_REQUEST_NONE);

pub fn request_chart(sensor_type: SensorType) {
    let val = match sensor_type {
        SensorType::Ph => CHART_REQUEST_PH,
        SensorType::Conductivity => CHART_REQUEST_EC,
        SensorType::Orp => CHART_REQUEST_ORP,
        SensorType::Temperature => CHART_REQUEST_TEMPERATURE,
    };
    CHART_REQUESTED.store(val, Ordering::Release);
}

pub fn take_chart_request() -> Option<SensorType> {
    let val = CHART_REQUESTED.swap(CHART_REQUEST_NONE, Ordering::AcqRel);
    match val {
        CHART_REQUEST_PH => Some(SensorType::Ph),
        CHART_REQUEST_EC => Some(SensorType::Conductivity),
        CHART_REQUEST_ORP => Some(SensorType::Orp),
        CHART_REQUEST_TEMPERATURE => Some(SensorType::Temperature),
        _ => None,
    }
}
