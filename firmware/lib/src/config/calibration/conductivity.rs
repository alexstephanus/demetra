use super::types::TimestampedValue;

use chrono::{DateTime, Utc};

use crate::{
    ui_types::ConductivityDisplayUnit,
    units::{Conductivity, Resistance, Temperature},
};

const EC_TEMP_COEFF: f32 = 0.02;
const EC_REF_TEMP_C: f32 = 25.0;

#[derive(Copy, Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ConductivityCalibration {
    pub cell_constant: f32,
    pub measured_resistance: Resistance,
    pub solution_us_per_cm: f32,
    pub calibration_temperature: Temperature,
    calibration_timestamp: DateTime<Utc>,
}

impl TimestampedValue for ConductivityCalibration {
    fn get_written_timestamp(&self) -> DateTime<Utc> {
        self.calibration_timestamp
    }
}

impl Default for ConductivityCalibration {
    fn default() -> Self {
        Self::new(
            Resistance::from_ohms(1000.0),
            1000.0,
            Temperature::from_celsius(25.0),
            DateTime::<Utc>::from_timestamp_millis(0).unwrap(),
        )
    }
}

impl ConductivityCalibration {
    pub fn new(
        measured_resistance: Resistance,
        solution_us_per_cm: f32,
        calibration_temperature: Temperature,
        calibration_timestamp: DateTime<Utc>,
    ) -> ConductivityCalibration {
        let r_at_25c = measured_resistance.ohms()
            * (1.0 + EC_TEMP_COEFF * (calibration_temperature.celsius() - EC_REF_TEMP_C));
        let cell_constant = solution_us_per_cm * r_at_25c / 1_000_000.0;
        ConductivityCalibration {
            cell_constant,
            measured_resistance,
            solution_us_per_cm,
            calibration_temperature,
            calibration_timestamp,
        }
    }

    pub fn get_conductivity(
        &self,
        measured_resistance: Resistance,
        measured_temperature: Temperature,
    ) -> Conductivity {
        let r_at_25c = measured_resistance.ohms()
            * (1.0 + EC_TEMP_COEFF * (measured_temperature.celsius() - EC_REF_TEMP_C));
        let ec_25 = self.cell_constant * 1_000_000.0 / r_at_25c;
        Conductivity::new(ec_25, ConductivityDisplayUnit::UsPerCm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{Resistance, Temperature};
    use chrono::DateTime;

    fn make_calibration(
        cal_resistance: f32,
        solution_us: f32,
        cal_temp_c: f32,
    ) -> ConductivityCalibration {
        ConductivityCalibration::new(
            Resistance::from_ohms(cal_resistance),
            solution_us,
            Temperature::from_celsius(cal_temp_c),
            DateTime::from_timestamp_millis(0).unwrap(),
        )
    }

    #[test]
    fn measurement_at_calibration_resistance_and_temperature_returns_solution_value() {
        let cal = make_calibration(1000.0, 1413.0, 25.0);
        let result = cal.get_conductivity(
            Resistance::from_ohms(1000.0),
            Temperature::from_celsius(25.0),
        );
        let diff = (result.us_per_cm() - 1413.0).abs();
        assert!(
            diff < 0.01,
            "Expected ~1413 µS/cm, got {:.4}",
            result.us_per_cm()
        );
    }

    #[test]
    fn higher_resistance_gives_lower_conductivity() {
        let cal = make_calibration(1000.0, 1413.0, 25.0);
        let low_r = cal.get_conductivity(
            Resistance::from_ohms(800.0),
            Temperature::from_celsius(25.0),
        );
        let high_r = cal.get_conductivity(
            Resistance::from_ohms(1200.0),
            Temperature::from_celsius(25.0),
        );
        assert!(low_r.us_per_cm() > high_r.us_per_cm());
    }

    #[test]
    fn temperature_compensation_higher_temp_reports_lower_ec() {
        let cal = make_calibration(1000.0, 1413.0, 25.0);
        let at_25 = cal.get_conductivity(
            Resistance::from_ohms(1000.0),
            Temperature::from_celsius(25.0),
        );
        let at_35 = cal.get_conductivity(
            Resistance::from_ohms(1000.0),
            Temperature::from_celsius(35.0),
        );
        assert!(
            at_35.us_per_cm() < at_25.us_per_cm(),
            "Higher temperature should yield lower compensated EC. 25°C: {:.2}, 35°C: {:.2}",
            at_25.us_per_cm(),
            at_35.us_per_cm()
        );
    }

    #[test]
    fn temperature_compensation_matches_formula() {
        let cal = make_calibration(1000.0, 1413.0, 25.0);
        let t_meas = 20.0_f32;
        let result = cal.get_conductivity(
            Resistance::from_ohms(1000.0),
            Temperature::from_celsius(t_meas),
        );
        let expected = 1413.0 * 1.0 / (1.0 + EC_TEMP_COEFF * (t_meas - EC_REF_TEMP_C));
        let diff = (result.us_per_cm() - expected).abs();
        assert!(
            diff < 0.01,
            "Expected {:.4}, got {:.4}",
            expected,
            result.us_per_cm()
        );
    }

    #[test]
    fn calibration_at_non_reference_temperature_compensates_correctly() {
        let cal_temp = 30.0_f32;
        let cal_resistance = 950.0_f32;
        let solution_us_25 = 1413.0_f32;
        let cal = make_calibration(cal_resistance, solution_us_25, cal_temp);
        let meas_temp = 20.0_f32;
        let result = cal.get_conductivity(
            Resistance::from_ohms(cal_resistance),
            Temperature::from_celsius(meas_temp),
        );
        let expected = solution_us_25 * (1.0 + EC_TEMP_COEFF * (cal_temp - EC_REF_TEMP_C))
            / (1.0 + EC_TEMP_COEFF * (meas_temp - EC_REF_TEMP_C));
        let diff = (result.us_per_cm() - expected).abs();
        assert!(
            diff < 0.01,
            "Expected {:.4}, got {:.4}",
            expected,
            result.us_per_cm()
        );
    }

    #[test]
    fn cell_constant_scales_correctly() {
        let cal_k_1 = make_calibration(1000.0, 1000.0, 25.0);
        let diff = (cal_k_1.cell_constant - 1.0).abs();
        assert!(
            diff < 0.01,
            "Expected K≈1.0, got {:.4}",
            cal_k_1.cell_constant
        );

        // Doubling the measured resistance should double the cell constant
        let cal_k_2 = make_calibration(2000.0, 1000.0, 25.0);
        let diff = (cal_k_2.cell_constant - 2.0).abs();
        assert!(
            diff < 0.01,
            "Expected K≈2.0, got {:.4}",
            cal_k_2.cell_constant
        );

        // Halving the measured resistance should halve the cell constant
        let cal_k_0_5 = make_calibration(500.0, 1000.0, 25.0);
        let diff = (cal_k_0_5.cell_constant - 0.5).abs();
        assert!(
            diff < 0.01,
            "Expected K≈0.5, got {:.4}",
            cal_k_0_5.cell_constant
        );
    }

    #[test]
    fn cell_constant_computed_correctly() {
        let cal = make_calibration(707.0, 1413.0, 25.0);
        let diff = (cal.cell_constant - 1.0).abs();
        assert!(diff < 0.01, "Expected K≈1.0, got {:.4}", cal.cell_constant);
    }
}
