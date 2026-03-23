use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Voltage {
    millivolts: f32,
}

impl Voltage {
    pub const fn from_mv(millivolts: f32) -> Self {
        Self { millivolts }
    }

    pub fn from_volts(volts: f32) -> Self {
        Self {
            millivolts: volts * 1000.0,
        }
    }

    pub fn mv(&self) -> f32 {
        self.millivolts
    }

    pub fn volts(&self) -> f32 {
        self.millivolts / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 0.001;

    fn assert_approx_eq(a: f32, b: f32) {
        assert!(
            (a - b).abs() < EPSILON,
            "Expected {}, got {}",
            a,
            b
        );
    }

    #[test]
    fn test_from_mv() {
        let v = Voltage::from_mv(3300.0);
        assert_eq!(v.mv(), 3300.0);
    }

    #[test]
    fn test_from_volts() {
        let v = Voltage::from_volts(3.3);
        assert_approx_eq(v.mv(), 3300.0);
    }

    #[test]
    fn test_mv_to_volts() {
        let v = Voltage::from_mv(1500.0);
        assert_approx_eq(v.volts(), 1.5);
    }

    #[test]
    fn test_volts_to_mv() {
        let v = Voltage::from_volts(1.5);
        assert_approx_eq(v.mv(), 1500.0);
    }

    #[test]
    fn test_roundtrip_mv() {
        let v = Voltage::from_mv(123.456);
        assert_eq!(v.mv(), 123.456);
    }

    #[test]
    fn test_roundtrip_volts() {
        let v = Voltage::from_volts(2.5);
        assert_approx_eq(Voltage::from_mv(v.mv()).volts(), 2.5);
    }

    #[test]
    fn test_zero() {
        let v = Voltage::from_mv(0.0);
        assert_eq!(v.mv(), 0.0);
        assert_eq!(v.volts(), 0.0);
    }

    #[test]
    fn test_negative_voltage() {
        let v = Voltage::from_mv(-200.0);
        assert_eq!(v.mv(), -200.0);
        assert_approx_eq(v.volts(), -0.2);
    }
}
