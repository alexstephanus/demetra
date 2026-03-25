mod cic_filter;
pub(crate) mod dosing;
mod outlets;
mod pumps;
use core::marker::PhantomData;

use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};

pub use crate::ui_types::{DosingPump, Pump};
pub use cic_filter::{cic_filter_order_3, InsufficientSamples, OversampleRatio};
pub use outlets::{OutletState, OutletStateList};
pub use pumps::{
    DosingPumpState, DosingPumpStateList, HardwarePumpController, PumpController, PumpError,
    CURRENT_CUTOFF,
};

pub mod rtc;

mod sensors;
pub use sensors::{SensorController, SensorError, SensorReadRaw};

pub type TreatmentControllerMutex<'a, Sensors, Pumps> =
    Mutex<NoopRawMutex, TreatmentController<'a, Sensors, Pumps>>;

/// This struct basically only exists so we can
/// stick everything behind a single mutex.
/// We don't want anything else running while sensors are running.
/// This minimized electrical interference and makes sure that,
/// for example, we aren't actively dosing the reservoir while
/// also taking measurements or stirring it.
///
/// It handles sensor reading and dosing, because that involves both
/// sensors and pumps, although realistically it would also be totally
/// possible to just do that via its own function.  We mostly do it here
/// just because we have easy access here to everything we need for it
pub struct TreatmentController<'a, Sensors: SensorReadRaw, Pumps: PumpController> {
    pub pump_controller: Pumps,
    pub sensor_controller: SensorController<'a, Sensors>,
    _phantom_lifetime: PhantomData<&'a str>,
}

impl<'a, Sensors: SensorReadRaw, Pumps: PumpController> TreatmentController<'a, Sensors, Pumps> {
    pub fn initialize(
        pump_controller: Pumps,
        sensor_controller: SensorController<'a, Sensors>,
    ) -> Self {
        TreatmentController {
            pump_controller,
            sensor_controller,
            _phantom_lifetime: PhantomData,
        }
    }
}
