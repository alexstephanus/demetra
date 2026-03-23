use crate::config::calibration::types::CalculateMidpoint;
use serde::{Deserialize, Serialize};

use crate::ui_types::ConductivityDisplayUnit;

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Conductivity {
    us_per_cm: f32,
}

impl Conductivity {
    pub fn from_us_per_cm(us_per_cm: f32) -> Self {
        Self { us_per_cm }
    }

    pub fn new(us_per_cm: f32, display_unit: ConductivityDisplayUnit) -> Self {
        Self {
            us_per_cm: match display_unit {
                ConductivityDisplayUnit::UsPerCm => us_per_cm,
                ConductivityDisplayUnit::Ppm500 => us_per_cm * 1000.0 / 500.0,
                ConductivityDisplayUnit::Ppm700 => us_per_cm * 1000.0 / 700.0,
            },
        }
    }

    pub fn us_per_cm(&self) -> f32 {
        self.us_per_cm
    }

    pub fn display(&self, display_unit: ConductivityDisplayUnit) -> f32 {
        match display_unit {
            ConductivityDisplayUnit::UsPerCm => self.us_per_cm,
            ConductivityDisplayUnit::Ppm500 => self.us_per_cm / 1000.0 * 500.0,
            ConductivityDisplayUnit::Ppm700 => self.us_per_cm / 1000.0 * 700.0,
        }
    }
}

impl CalculateMidpoint for Conductivity {
    fn get_midpoint(&self, other: &Self) -> Self {
        Self {
            us_per_cm: (self.us_per_cm + other.us_per_cm) / 2.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        assert_eq!(
            Conductivity::new(10.0, ConductivityDisplayUnit::UsPerCm),
            Conductivity { us_per_cm: 10.0 }
        );
    }

    #[test]
    fn test_us_per_cm() {
        assert_eq!(
            Conductivity::new(10.0, ConductivityDisplayUnit::UsPerCm).us_per_cm(),
            10.0
        );
    }

    #[test]
    fn test_display_us_per_cm() {
        assert_eq!(
            Conductivity::new(10.0, ConductivityDisplayUnit::UsPerCm)
                .display(ConductivityDisplayUnit::UsPerCm),
            10.0
        );
    }

    #[test]
    fn test_display_ppm500() {
        assert_eq!(
            Conductivity::new(10.0, ConductivityDisplayUnit::UsPerCm)
                .display(ConductivityDisplayUnit::Ppm500),
            5.0
        );
    }

    #[test]
    fn test_display_ppm700() {
        assert_eq!(
            Conductivity::new(100.0, ConductivityDisplayUnit::UsPerCm)
                .display(ConductivityDisplayUnit::Ppm700),
            70.0
        );
    }
}
