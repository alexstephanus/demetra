use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::units::Voltage;

use super::types::TimestampedValue;

#[derive(Copy, Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OrpMeasurementPoint {
    pub voltage: Voltage,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OrpCalibration {
    calibrated_offset: Voltage,
    slope: f32,
    calibration_timestamp: DateTime<Utc>,
}

impl TimestampedValue for OrpCalibration {
    fn get_written_timestamp(&self) -> DateTime<Utc> {
        self.calibration_timestamp
    }
}

impl Default for OrpCalibration {
    fn default() -> Self {
        Self::new(Voltage::from_mv(0.0), Voltage::from_mv(0.0), DateTime::<Utc>::from_timestamp_millis(0).unwrap())
    }
}

impl OrpCalibration {
    pub fn new(
        measured_voltage: Voltage,
        solution_voltage: Voltage,
        calibration_timestamp: DateTime<Utc>,
    ) -> OrpCalibration {
        let mv_positive = measured_voltage.mv() >= 0.0;
        let solution_positive = solution_voltage.mv() >= 0.0;

        // Accounts for the sensor potentially being wired incorrectly
        let slope: f32 = match mv_positive == solution_positive {
            true => 1.0,
            false => -1.0,
        };

        let calibrated_offset = Voltage::from_mv((measured_voltage.mv() * slope) - solution_voltage.mv());

        OrpCalibration {
            calibrated_offset,
            slope,
            calibration_timestamp,
        }
    }

    pub fn get_calibrated_orp_measurement(
        &self,
        measured_voltage: Voltage,
    ) -> OrpMeasurementPoint {
        let voltage = Voltage::from_mv(measured_voltage.mv() * self.slope - self.calibrated_offset.mv());
        OrpMeasurementPoint { voltage }
    }
}
