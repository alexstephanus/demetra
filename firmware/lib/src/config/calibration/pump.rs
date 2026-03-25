use chrono::{DateTime, Utc};
use embassy_time::Duration;

use crate::units::Volume;

use super::types::TimestampedValue;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct DoseCalibrationPoint {
    pub dispense_duration_ms: f32,
    pub ml_dispensed: f32,
}

impl DoseCalibrationPoint {
    pub fn new(dispense_duration_ms: f32, ml_dispensed: f32) -> Self {
        Self {
            dispense_duration_ms,
            ml_dispensed,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct DosingPumpCalibration {
    ms_per_ml: f32,
    ms_to_start: f32,
    calibration_timestamp: DateTime<Utc>,
}

impl TimestampedValue for DosingPumpCalibration {
    fn get_written_timestamp(&self) -> DateTime<Utc> {
        self.calibration_timestamp
    }
}

impl DosingPumpCalibration {
    pub const fn default() -> Self {
        Self {
            // TODO Add a sane default
            ms_per_ml: 400.0,
            ms_to_start: 0.0,
            calibration_timestamp: DateTime::<Utc>::from_timestamp_millis(0).unwrap(),
        }
    }

    pub fn new(
        first_dose: DoseCalibrationPoint,
        second_dose: DoseCalibrationPoint,
        third_dose: DoseCalibrationPoint,
        calibration_time: DateTime<Utc>,
    ) -> Self {
        let dose_volume_avg =
            (first_dose.ml_dispensed + second_dose.ml_dispensed + third_dose.ml_dispensed) / 3.0;
        let dose_millis_avg = (first_dose.dispense_duration_ms
            + second_dose.dispense_duration_ms
            + third_dose.dispense_duration_ms)
            / 3.0;
        let beta_numerator = (first_dose.dispense_duration_ms - dose_millis_avg)
            * (first_dose.ml_dispensed - dose_volume_avg)
            + (second_dose.dispense_duration_ms - dose_millis_avg)
                * (second_dose.ml_dispensed - dose_volume_avg)
            + (third_dose.dispense_duration_ms - dose_millis_avg)
                * (third_dose.ml_dispensed - dose_volume_avg);
        let beta_denominator = (first_dose.dispense_duration_ms - dose_millis_avg)
            * (first_dose.dispense_duration_ms - dose_millis_avg)
            + (second_dose.dispense_duration_ms - dose_millis_avg)
                * (second_dose.dispense_duration_ms - dose_millis_avg)
            + (third_dose.dispense_duration_ms - dose_millis_avg)
                * (third_dose.dispense_duration_ms - dose_millis_avg);

        let ms_per_ml = beta_denominator / beta_numerator;

        let ms_to_start = dose_volume_avg - (dose_millis_avg / ms_per_ml);

        Self {
            ms_per_ml,
            ms_to_start,
            calibration_timestamp: calibration_time,
        }
    }

    pub fn from_calibration_values(
        ms_per_ml: f32,
        ms_to_start: f32,
        calibration_timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            ms_per_ml,
            ms_to_start,
            calibration_timestamp,
        }
    }

    pub fn get_dose_duration(&self, volume: Volume) -> Duration {
        Duration::from_millis((self.ms_per_ml * volume.to_milliliters() + self.ms_to_start) as u64)
    }
}

#[cfg(test)]
mod tests {
    use crate::config::calibration::{DoseCalibrationPoint, DosingPumpCalibration};
    use chrono::{DateTime, Utc};

    #[test]
    fn test_calibration() {
        assert_eq!(
            DosingPumpCalibration::new(
                DoseCalibrationPoint::new(1000.0, 1.0),
                DoseCalibrationPoint::new(2000.0, 2.0),
                DoseCalibrationPoint::new(3000.0, 3.0),
                DateTime::<Utc>::from_timestamp_millis(0).unwrap(),
            ),
            DosingPumpCalibration::from_calibration_values(
                1000.0,
                0.0,
                DateTime::<Utc>::from_timestamp_millis(0).unwrap(),
            )
        )
    }
}
