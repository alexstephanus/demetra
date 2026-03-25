
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::units::Temperature;

use crate::units::Voltage;

use super::types::TimestampedValue;

pub type PhValue = f32;

const IDEAL_PH_TEMPERATURE_CELSIUS: f32 = 25.0;
const IDEAL_PH_SLOPE: f32 = 59.15; // mV per pH unit.  Sloping downwards

const NEUTRAL_PH: PhValue = 7.00; // duh

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct PhMeasurementPoint {
    pub ph_value: PhValue,
    measured_voltage: Voltage,
    temperature: Temperature,
}

impl PhMeasurementPoint {
    pub fn new(
        ph_value: PhValue,
        measured_voltage: Voltage,
        temperature: Temperature,
    ) -> PhMeasurementPoint {
        PhMeasurementPoint {
            ph_value,
            measured_voltage,
            temperature,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct ThreePointPhCalibration {
    high: PhMeasurementPoint,
    mid: PhMeasurementPoint,
    low: PhMeasurementPoint,
    calibration_time: DateTime<Utc>,
    pub slope_high: f32, // the slope used above ph 7.0, at the ideal 25C temp
    pub slope_low: f32,  // the slope used below 7.0, also at 25C
    pub mv_offset: Voltage, // the offset at 7.0.  This is temp independent
}

impl TimestampedValue for ThreePointPhCalibration {
    fn get_written_timestamp(&self) -> DateTime<Utc> {
        self.calibration_time
    }
}

impl Default for ThreePointPhCalibration {
    fn default() -> Self {
        Self::new(
            PhMeasurementPoint::new(10.00, Voltage::from_mv(-57.0 * 3.0), Temperature::from_celsius(25.0)),
            PhMeasurementPoint::new(7.00, Voltage::from_mv(0.0), Temperature::from_celsius(25.0)),
            PhMeasurementPoint::new(4.00, Voltage::from_mv(57.0 * 3.0), Temperature::from_celsius(25.0)),
            DateTime::<Utc>::from_timestamp_millis(0).unwrap(),
        )
    }
}

impl ThreePointPhCalibration {
    pub fn new(
        high: PhMeasurementPoint,
        mid: PhMeasurementPoint,
        low: PhMeasurementPoint,
        calibration_time: DateTime<Utc>,
    ) -> ThreePointPhCalibration {
        let (slope_high, slope_low, mv_offset) =
            ThreePointPhCalibration::calculate_slope_and_offset(&high, &mid, &low);
        ThreePointPhCalibration {
            high,
            mid,
            low,
            calibration_time,
            slope_high,
            slope_low,
            mv_offset,
        }
    }

    fn calculate_slope_and_offset(
        high: &PhMeasurementPoint,
        mid: &PhMeasurementPoint,
        low: &PhMeasurementPoint,
    ) -> (f32, f32, Voltage) {
        // We need to find the midpoint while accounting for temperature
        let ideal_temperature = Temperature::from_celsius(IDEAL_PH_TEMPERATURE_CELSIUS);
        if mid.ph_value <= NEUTRAL_PH {
            let slope_high = ideal_temperature.kelvin() * (high.measured_voltage.mv() - mid.measured_voltage.mv())
                / ((high.ph_value * high.temperature.kelvin())
                    - NEUTRAL_PH * (high.temperature.kelvin() - mid.temperature.kelvin())
                    - (mid.ph_value * mid.temperature.kelvin()));
            let calibration_offset = Voltage::from_mv(mid.measured_voltage.mv()
                + (slope_high * mid.temperature.kelvin() / ideal_temperature.kelvin()
                    * (NEUTRAL_PH - mid.ph_value)));
            let slope_low = ideal_temperature.kelvin() * (calibration_offset.mv() - low.measured_voltage.mv())
                / (low.temperature.kelvin() * (NEUTRAL_PH - low.ph_value));
            (slope_high, slope_low, calibration_offset)
        } else {
            let slope_low = ideal_temperature.kelvin() * (mid.measured_voltage.mv() - low.measured_voltage.mv())
                / ((mid.ph_value * mid.temperature.kelvin())
                    - NEUTRAL_PH * (mid.temperature.kelvin() - low.temperature.kelvin())
                    - (low.ph_value * low.temperature.kelvin()));
            let calibration_offset = Voltage::from_mv(mid.measured_voltage.mv()
                - (slope_low * mid.temperature.kelvin() / ideal_temperature.kelvin()
                    * (mid.ph_value - NEUTRAL_PH)));
            let slope_high = (high.measured_voltage.mv() - calibration_offset.mv()) * ideal_temperature.kelvin()
                / (high.temperature.kelvin() * (high.ph_value - NEUTRAL_PH));
            (slope_high, slope_low, calibration_offset)
        }
    }

    pub fn get_calibrated_ph_measurement(
        &self,
        temperature: Temperature,
        measured_voltage: Voltage,
    ) -> PhMeasurementPoint {
        let ideal_temperature = Temperature::from_celsius(IDEAL_PH_TEMPERATURE_CELSIUS);
        let temperature_compensated_slope = match measured_voltage.mv() < self.mv_offset.mv() {
            true => self.slope_high * temperature.kelvin() / ideal_temperature.kelvin(),
            false => self.slope_low * temperature.kelvin() / ideal_temperature.kelvin(),
        };

        let ph_value: PhValue = NEUTRAL_PH + (measured_voltage.mv() - self.mv_offset.mv()) / temperature_compensated_slope;

        PhMeasurementPoint::new(
            ph_value,
            measured_voltage,
            temperature,
        )
    }

    /// Calculate the slope percentage compared to ideal pH probe performance
    /// Returns the average slope as a percentage of the ideal slope (59.15 mV/pH)
    /// A healthy probe should return 95-105%
    pub fn slope_percentage(&self) -> f32 {
        let average_slope = (self.slope_high.abs() + self.slope_low.abs()) / 2.0;
        (average_slope / IDEAL_PH_SLOPE) * 100.0
    }
}

#[cfg(test)]
mod test_ph_calibration {
    use crate::units::{Temperature, Voltage};

    use super::{PhMeasurementPoint, ThreePointPhCalibration};

    const FLOAT_EPSILON: f32 = 0.00001;

    fn assert_float_equality(float1: f32, float2: f32) {
        assert!(
            float1 - float2 < FLOAT_EPSILON && float1 - float2 > -FLOAT_EPSILON,
            "f1: {}, f2: {}",
            float1,
            float2
        )
    }

    #[test]
    fn test_ph_calibration_standard() {
        let low_ph = PhMeasurementPoint::new(4.00, Voltage::from_mv(57.0 * 3.0), Temperature::from_celsius(25.0));
        let mid_ph = PhMeasurementPoint::new(7.00, Voltage::from_mv(0.0), Temperature::from_celsius(25.0));
        let high_ph = PhMeasurementPoint::new(10.00, Voltage::from_mv(-57.0 * 3.0), Temperature::from_celsius(25.0));
        let res = ThreePointPhCalibration::calculate_slope_and_offset(&high_ph, &mid_ph, &low_ph);
        assert_float_equality(res.0, -57.0);
        assert_float_equality(res.1, -57.0);
        assert_float_equality(res.2.mv(), 0.0);
    }

    #[test]
    fn test_ph_calibration_6_mid() {
        let low_ph = PhMeasurementPoint::new(4.00, Voltage::from_mv(57.0 * 3.0), Temperature::from_celsius(25.0));
        let mid_ph = PhMeasurementPoint::new(6.00, Voltage::from_mv(57.0), Temperature::from_celsius(25.0));
        let high_ph = PhMeasurementPoint::new(10.00, Voltage::from_mv(-57.0 * 3.0), Temperature::from_celsius(25.0));
        let res = ThreePointPhCalibration::calculate_slope_and_offset(&high_ph, &mid_ph, &low_ph);
        assert_float_equality(res.0, -57.0);
        assert_float_equality(res.1, -57.0);
        assert_float_equality(res.2.mv(), 0.0);
    }

    #[test]
    fn test_ph_calibration_8_mid() {
        let low_ph = PhMeasurementPoint::new(4.00, Voltage::from_mv(57.0 * 3.0), Temperature::from_celsius(25.0));
        let mid_ph = PhMeasurementPoint::new(8.00, Voltage::from_mv(-57.0), Temperature::from_celsius(25.0));
        let high_ph = PhMeasurementPoint::new(10.00, Voltage::from_mv(-57.0 * 3.0), Temperature::from_celsius(25.0));

        let res = ThreePointPhCalibration::calculate_slope_and_offset(&high_ph, &mid_ph, &low_ph);
        assert_float_equality(res.0, -57.0);
        assert_float_equality(res.1, -57.0);
        assert_float_equality(res.2.mv(), 0.0);
    }

    fn standard_calibration() -> ThreePointPhCalibration {
        use chrono::DateTime;
        ThreePointPhCalibration::new(
            PhMeasurementPoint::new(10.00, Voltage::from_mv(-57.0 * 3.0), Temperature::from_celsius(25.0)),
            PhMeasurementPoint::new(7.00, Voltage::from_mv(0.0), Temperature::from_celsius(25.0)),
            PhMeasurementPoint::new(4.00, Voltage::from_mv(57.0 * 3.0), Temperature::from_celsius(25.0)),
            DateTime::from_timestamp_millis(0).unwrap(),
        )
    }

    #[test]
    fn test_measurement_at_calibration_points() {
        let cal = standard_calibration();
        let t = Temperature::from_celsius(25.0);
        assert_float_equality(cal.get_calibrated_ph_measurement(t, Voltage::from_mv(57.0 * 3.0)).ph_value, 4.0);
        assert_float_equality(cal.get_calibrated_ph_measurement(t, Voltage::from_mv(0.0)).ph_value, 7.0);
        assert_float_equality(cal.get_calibrated_ph_measurement(t, Voltage::from_mv(-57.0 * 3.0)).ph_value, 10.0);
    }

    #[test]
    fn test_measurement_temperature_compensation() {
        let cal = standard_calibration();
        let t_measured = Temperature::from_celsius(15.0);
        let t_ideal = Temperature::from_celsius(25.0);

        let low_ph_mv = 57.0 * 3.0_f32;
        let expected_low = 7.0 + low_ph_mv / (cal.slope_low * t_measured.kelvin() / t_ideal.kelvin());
        assert_float_equality(
            cal.get_calibrated_ph_measurement(t_measured, Voltage::from_mv(low_ph_mv)).ph_value,
            expected_low,
        );

        let high_ph_mv = -57.0 * 3.0_f32;
        let expected_high = 7.0 + high_ph_mv / (cal.slope_high * t_measured.kelvin() / t_ideal.kelvin());
        assert_float_equality(
            cal.get_calibrated_ph_measurement(t_measured, Voltage::from_mv(high_ph_mv)).ph_value,
            expected_high,
        );
    }
}
