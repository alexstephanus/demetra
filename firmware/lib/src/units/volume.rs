const GALLONS_TO_LITERS: f32 = 3.78541;
const LITERS_TO_GALLONS: f32 = 1.0 / GALLONS_TO_LITERS;

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct Volume {
    liters: f32,
}

impl Volume {
    pub const fn from_liters(liters: f32) -> Self {
        Self { liters }
    }

    pub fn from_gallons(gallons: f32) -> Self {
        Self {
            liters: gallons * GALLONS_TO_LITERS,
        }
    }

    pub fn from_milliliters(milliliters: f32) -> Self {
        Self {
            liters: milliliters / 1000.0,
        }
    }
}

impl Volume {
    pub fn to_liters(&self) -> f32 {
        self.liters
    }

    pub fn to_gallons(&self) -> f32 {
        self.liters * LITERS_TO_GALLONS
    }

    pub fn to_milliliters(&self) -> f32 {
        self.liters * 1000.0
    }
}

// TODO: Use better floating-point comparisons
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_liters() {
        assert_eq!(Volume::from_liters(1.0), Volume { liters: 1.0 });
    }

    #[test]
    fn test_from_gallons() {
        assert_eq!(Volume::from_gallons(1.0), Volume { liters: 3.78541 });
    }

    #[test]
    fn test_from_milliliters() {
        assert_eq!(Volume::from_milliliters(1.0), Volume { liters: 0.001 });
    }

    #[test]
    fn test_to_liters() {
        assert_eq!(Volume::from_liters(1.0).to_liters(), 1.0);
    }

    #[test]
    fn test_to_gallons() {
        assert_eq!(Volume::from_liters(1.0).to_gallons(), 0.2641722);
    }

    #[test]
    fn test_to_milliliters() {
        assert_eq!(Volume::from_liters(1.0).to_milliliters(), 1000.0);
    }
}
