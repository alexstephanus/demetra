use chrono::{DateTime, Utc};
use core::fmt::Debug;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    mutex::Mutex,
    rwlock::RwLock,
};
use embedded_storage::Storage;

use crate::{
    config::device_config::DeviceConfig,
    storage::ring_buffer::{EmptyMetadata, RingBuffer},
};

type LockedState<T> = RwLock<CriticalSectionRawMutex, T>;

static SYSTEM_CONFIG: LockedState<DeviceConfig> = RwLock::new(DeviceConfig::default());

static CONFIG_WRITE_SERIALIZER: Mutex<CriticalSectionRawMutex, ()> = Mutex::new(());

pub async fn get_device_config() -> DeviceConfig {
    (*SYSTEM_CONFIG.read().await).clone()
}

pub async fn set_device_config(config: DeviceConfig) {
    *SYSTEM_CONFIG.write().await = config;
}

pub async fn update_device_config<S: Storage<Error = E>, E: Debug>(
    buffer: &mut RingBuffer<DeviceConfig, EmptyMetadata, S, E>,
    write_timestamp: DateTime<Utc>,
    update_fn: impl FnOnce(&mut DeviceConfig),
) {
    let _write_guard = CONFIG_WRITE_SERIALIZER.lock().await;

    let cloned_device_config = {
        let config = SYSTEM_CONFIG.read().await;
        let mut cloned = config.clone();
        update_fn(&mut cloned);
        cloned.configuration_written_at = write_timestamp;
        cloned
    };

    if let Err(e) = buffer.write_record(&cloned_device_config, EmptyMetadata) {
        log::error!("Failed to write config to flash storage: {}", e);
        return;
    }

    let written_record = match buffer.read_latest_record() {
        Ok(Some((record, _metadata))) => record,
        Ok(None) => {
            log::error!("Failed to read latest record from flash storage after write");
            return;
        }
        Err(e) => {
            log::error!("Error reading config from flash storage: {}", e);
            return;
        }
    };

    if written_record != cloned_device_config {
        log::error!("Failed to successfully write new config to flash storage - data mismatch");
        return;
    }
    *SYSTEM_CONFIG.write().await = written_record;
}
