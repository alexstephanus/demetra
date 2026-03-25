use chrono::{DateTime, Utc};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, once_lock::OnceLock, rwlock::RwLock,
};

use crate::peripherals::rtc::SystemTimeInfo;

type LockedState<T> = RwLock<CriticalSectionRawMutex, T>;

static CLOCK_FN: OnceLock<fn() -> u64> = OnceLock::new();

pub fn init_clock(f: fn() -> u64) -> Result<(), fn() -> u64> {
    CLOCK_FN.init(f)
}

pub(crate) fn current_micros() -> u64 {
    let f = *CLOCK_FN.try_get().expect("Clock not initialized");
    f()
}

static SYSTEM_TIME_INFO: LockedState<SystemTimeInfo> = RwLock::new(SystemTimeInfo::default());

pub async fn get_system_time(micros: u64) -> DateTime<Utc> {
    let system_time_info = *SYSTEM_TIME_INFO.read().await;
    system_time_info.get_current_time(micros)
}

pub async fn read_system_time_info() -> SystemTimeInfo {
    *SYSTEM_TIME_INFO.read().await
}

pub async fn set_system_time_info(info: SystemTimeInfo) {
    *SYSTEM_TIME_INFO.write().await = info;
}
