use core::cell::RefCell;

use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use esp_hal::{i2c::master::I2c, Blocking};

pub type SharedI2cDevice<'a> = I2cDevice<'a, NoopRawMutex, I2c<'a, Blocking>>;

pub type McpOutputPin<'a> = port_expander::Pin<
    'a,
    port_expander::mode::Output,
    RefCell<
        port_expander::dev::mcp23x17::Driver<
            port_expander::dev::mcp23x17::Mcp23017Bus<SharedI2cDevice<'a>>,
        >,
    >,
>;
