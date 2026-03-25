use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};

use esp_hal::gpio::Output;

use lib::peripherals::{HardwarePumpController, TreatmentController};

use super::{
    type_aliases::{McpOutputPin, SharedI2cDevice},
    Sensors_1_0_0,
};

pub type EspTreatmentController<'a> = TreatmentController<
    'a,
    Sensors_1_0_0<'a>,
    HardwarePumpController<
        'a,
        McpOutputPin<'a>,
        McpOutputPin<'a>,
        SharedI2cDevice<'a>,
        McpOutputPin<'a>,
        Output<'a>,
    >,
>;

pub type EspTreatmentControllerMutex<'a> = Mutex<NoopRawMutex, EspTreatmentController<'a>>;
