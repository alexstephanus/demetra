use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Resistance {
    ohms: f32,
}

impl Resistance {
    pub const fn from_ohms(ohms: f32) -> Self {
        Self { ohms }
    }

    pub fn from_kilohms(kilohms: f32) -> Self {
        Self {
            ohms: kilohms * 1000.0,
        }
    }

    pub fn ohms(&self) -> f32 {
        self.ohms
    }

    pub fn kilohms(&self) -> f32 {
        self.ohms / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 0.001;

    fn assert_approx_eq(a: f32, b: f32) {
        assert!((a - b).abs() < EPSILON, "Expected {}, got {}", a, b);
    }

    #[test]
    fn test_from_ohms() {
        let r = Resistance::from_ohms(10000.0);
        assert_eq!(r.ohms(), 10000.0);
    }

    #[test]
    fn test_from_kilohms() {
        let r = Resistance::from_kilohms(10.0);
        assert_approx_eq(r.ohms(), 10000.0);
    }

    #[test]
    fn test_ohms_to_kilohms() {
        let r = Resistance::from_ohms(4700.0);
        assert_approx_eq(r.kilohms(), 4.7);
    }

    #[test]
    fn test_kilohms_to_ohms() {
        let r = Resistance::from_kilohms(4.7);
        assert_approx_eq(r.ohms(), 4700.0);
    }

    #[test]
    fn test_roundtrip_ohms() {
        let r = Resistance::from_ohms(123.456);
        assert_eq!(r.ohms(), 123.456);
    }

    #[test]
    fn test_roundtrip_kilohms() {
        let r = Resistance::from_kilohms(2.2);
        assert_approx_eq(Resistance::from_ohms(r.ohms()).kilohms(), 2.2);
    }

    #[test]
    fn test_zero() {
        let r = Resistance::from_ohms(0.0);
        assert_eq!(r.ohms(), 0.0);
        assert_eq!(r.kilohms(), 0.0);
    }
}
