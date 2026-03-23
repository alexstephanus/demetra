use chrono::{DateTime, Utc};
use core::fmt::Debug;

pub use crate::units::{Resistance, Voltage};

pub trait TimestampedValue {
    fn get_written_timestamp(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RangePosition {
    Low,
    Settled,
    High,
}

#[derive(Clone, Copy, Debug)]
pub struct NumericRange<T: Debug> {
    low: T,
    high: T,
}

pub trait CalculateMidpoint {
    fn get_midpoint(&self, other: &Self) -> Self;
}

impl CalculateMidpoint for f32 {
    fn get_midpoint(&self, other: &Self) -> Self {
        (self + other) / 2.0
    }
}

impl<T: Copy + CalculateMidpoint + Debug + PartialOrd + PartialEq> NumericRange<T> {
    pub fn new(low: T, high: T) -> Self {
        Self { low, high }
    }

    pub fn contains(&self, t: &T) -> bool {
        t >= &self.low && t <= &self.high
    }

    pub fn midpoint(&self) -> T {
        self.low.get_midpoint(&self.high)
    }

    pub fn low(&self) -> T {
        self.low
    }

    pub fn high(&self) -> T {
        self.high
    }
}

const DEFAULT_INNER_MARGIN: f32 = 0.2;

impl NumericRange<f32> {
    pub fn position(&self, value: f32) -> RangePosition {
        self.position_with_margin(value, DEFAULT_INNER_MARGIN)
    }

    pub fn position_with_margin(&self, value: f32, margin: f32) -> RangePosition {
        let span = self.high - self.low;
        let inner_low = self.low + span * margin;
        let inner_high = self.high - span * margin;
        if value < inner_low {
            RangePosition::Low
        } else if value > inner_high {
            RangePosition::High
        } else {
            RangePosition::Settled
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_below_outer_range_is_low() {
        let range = NumericRange::new(6.0, 8.0);
        assert_eq!(range.position(5.0), RangePosition::Low);
    }

    #[test]
    fn position_above_outer_range_is_high() {
        let range = NumericRange::new(6.0, 8.0);
        assert_eq!(range.position(9.0), RangePosition::High);
    }

    #[test]
    fn position_at_midpoint_is_settled() {
        let range = NumericRange::new(6.0, 8.0);
        assert_eq!(range.position(7.0), RangePosition::Settled);
    }

    #[test]
    fn position_in_lower_margin_is_low() {
        let range = NumericRange::new(6.0, 8.0);
        // inner_low = 6.0 + 2.0 * 0.2 = 6.4
        assert_eq!(range.position(6.3), RangePosition::Low);
    }

    #[test]
    fn position_in_upper_margin_is_high() {
        let range = NumericRange::new(6.0, 8.0);
        // inner_high = 8.0 - 2.0 * 0.2 = 7.6
        assert_eq!(range.position(7.7), RangePosition::High);
    }

    #[test]
    fn position_at_inner_low_boundary_is_settled() {
        let range = NumericRange::new(6.0, 8.0);
        assert_eq!(range.position(6.4), RangePosition::Settled);
    }

    #[test]
    fn position_at_inner_high_boundary_is_settled() {
        let range = NumericRange::new(6.0, 8.0);
        assert_eq!(range.position(7.6), RangePosition::Settled);
    }

    #[test]
    fn position_custom_margin() {
        let range = NumericRange::new(0.0, 100.0);
        // margin 0.1 → inner band is 10..90
        assert_eq!(range.position_with_margin(5.0, 0.1), RangePosition::Low);
        assert_eq!(range.position_with_margin(50.0, 0.1), RangePosition::Settled);
        assert_eq!(range.position_with_margin(95.0, 0.1), RangePosition::High);
    }

    #[test]
    fn position_zero_margin_matches_contains() {
        let range = NumericRange::new(6.0, 8.0);
        assert_eq!(range.position_with_margin(6.0, 0.0), RangePosition::Settled);
        assert_eq!(range.position_with_margin(8.0, 0.0), RangePosition::Settled);
        assert_eq!(range.position_with_margin(5.99, 0.0), RangePosition::Low);
        assert_eq!(range.position_with_margin(8.01, 0.0), RangePosition::High);
    }
}
