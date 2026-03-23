pub mod device_config;
pub mod ring_buffer;

pub use device_config::{get_device_config, set_device_config, update_device_config};
pub use ring_buffer::{EmptyMetadata, RingBuffer, RingBufferError};

// The address values (0x______) need to be kept in sync
// with the partition table.  It's not ideal to have this done manually,
// But it is for now.  These shouldn't have to change often, if ever.

pub const CONFIGS_START_ADDRESS: u32 = 0x410000;
pub const CONFIGS_END_ADDRESS: u32 = 0x430000;

pub const LOGS_START_ADDRESS: u32 = 0x500000;
pub const LOGS_END_ADDRESS: u32 = 0x800000;
