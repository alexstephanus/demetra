use log::debug;

use super::Resistance;
use libm::logf;
use serde::{Deserialize, Serialize};

const KELVIN_FROM_CELSIUS: f32 = 273.15;

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone, Copy)]
pub enum TemperatureDisplayUnit {
    Celsius,
    Fahrenheit,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Temperature {
    celsius: f32,
}

impl Temperature {
    pub fn from_resistance(resistance: Resistance, beta: f32) -> Temperature {
        let proportional_resistance = resistance.ohms() / 10000.0;
        let inverse_t1 =
            (logf(proportional_resistance) / beta) + (1.0 / (25.0 + KELVIN_FROM_CELSIUS));
        debug!("Proportional resistance: {:?}", proportional_resistance);
        let t1_celsius = (1.0 / inverse_t1) - 273.15;
        Temperature {
            celsius: t1_celsius,
        }
    }

    pub fn from_celsius(celsius: f32) -> Temperature {
        Temperature { celsius }
    }

    pub fn from_fahrenheit(fahrenheit: f32) -> Temperature {
        Temperature {
            celsius: (fahrenheit - 32.0) / 9.0 * 5.0,
        }
    }

    pub fn celsius(&self) -> f32 {
        self.celsius
    }

    pub fn fahrenheit(&self) -> f32 {
        self.celsius * 9.0 / 5.0 + 32.0
    }

    pub fn kelvin(&self) -> f32 {
        self.celsius + 273.15
    }

    pub fn display(&self, display_unit: TemperatureDisplayUnit) -> f32 {
        match display_unit {
            TemperatureDisplayUnit::Celsius => self.celsius(),
            TemperatureDisplayUnit::Fahrenheit => self.fahrenheit(),
        }
    }
}

impl Default for Temperature {
    fn default() -> Self {
        Temperature::from_celsius(25.0)
    }
}

// TODO: Use better floating-point comparisons
#[cfg(test)]
mod tests {
    use proptest::{
        prop_assert,
        proptest,
    };
    use super::*;
    use crate::units::Resistance;

    proptest! {
        #[test]
        fn test_from_resistance_with_beta_25_c(beta in 1.0f32..50000.0f32) {
            let computed_temperature = Temperature::from_resistance(Resistance::from_ohms(10000.0), beta);
            let expected_temperature = Temperature::from_celsius(25.0);
            prop_assert!(
                (computed_temperature.celsius() - expected_temperature.celsius()).abs() < 0.01,
                "Resistance of 10k should produce 25C for any beta, got {:?} for beta {:?}",
                computed_temperature.celsius(),
                beta
            )
        }
    }

    #[rstest::rstest]
    #[case(15.0, 15_885.15)]
    #[case(20.0, 12_553.985)]
    #[case(25.0, 10_000.0)]
    #[case(30.0, 8_025.59)]
    #[case(35.0, 6_487.15)]
    fn test_from_resistance_beta_3976(#[case] expected_temperature: f32, #[case] resistance: f32) {
        let computed_temperature = Temperature::from_resistance(Resistance::from_ohms(resistance), 3976.0);
        assert!(
            (computed_temperature.celsius() - expected_temperature).abs() < 0.01,
            "Resistance of {:?} should produce {:?}C for beta 3976, got {:?}",
            resistance,
            expected_temperature,
            computed_temperature.celsius()
        );
    }

    #[test]
    fn test_from_celsius() {
        assert_eq!(
            Temperature::from_celsius(25.0),
            Temperature { celsius: 25.0 }
        );
    }

    #[test]
    fn test_from_fahrenheit() {
        assert_eq!(
            Temperature::from_fahrenheit(77.0),
            Temperature { celsius: 25.0 }
        );
    }

    #[test]
    fn test_celsius() {
        assert_eq!(Temperature::from_celsius(25.0).celsius(), 25.0);
    }

    #[test]
    fn test_fahrenheit() {
        assert_eq!(Temperature::from_celsius(25.0).fahrenheit(), 77.0);
    }

    #[test]
    fn test_kelvin() {
        assert_eq!(Temperature::from_celsius(25.0).kelvin(), 298.15);
    }

    #[test]
    fn test_display_celsius() {
        assert_eq!(
            Temperature::from_celsius(25.0).display(TemperatureDisplayUnit::Celsius),
            25.0
        );
    }

    #[test]
    fn test_display_fahrenheit() {
        assert_eq!(
            Temperature::from_celsius(25.0).display(TemperatureDisplayUnit::Fahrenheit),
            77.0
        );
    }
}
