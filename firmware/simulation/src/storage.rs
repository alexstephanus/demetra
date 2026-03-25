use anyhow::Result;
use lib::data::configuration::DeviceConfig;
use lib::storage::ring_buffer::{MockFlashStorage, MockFlashStorageError, SimpleRingBuffer};

pub fn create_config_storage(
) -> Result<SimpleRingBuffer<DeviceConfig, MockFlashStorage, MockFlashStorageError>> {
    let start_address = 0x0000;
    let end_address = 0x4000; // 16KB for config storage
    let mock_flash = MockFlashStorage::new(start_address, end_address, None);

    Ok(
        SimpleRingBuffer::new(start_address, end_address, mock_flash)
            .expect("simulation storage addresses must be page-aligned"),
    )
}
