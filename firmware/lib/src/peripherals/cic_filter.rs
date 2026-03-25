use core::iter::IntoIterator;

#[derive(Clone, Copy)]
pub enum OversampleRatio {
    _4 = 4,
    _8 = 8,
    _16 = 16,
    _32 = 32,
    _64 = 64,
    _128 = 128,
    _256 = 256,
    _512 = 512,
    _1024 = 1024,
}

impl OversampleRatio {
    const fn log_base_2(&self) -> u32 {
        match self {
            OversampleRatio::_4 => 2,
            OversampleRatio::_8 => 3,
            OversampleRatio::_16 => 4,
            OversampleRatio::_32 => 5,
            OversampleRatio::_64 => 6,
            OversampleRatio::_128 => 7,
            OversampleRatio::_256 => 8,
            OversampleRatio::_512 => 9,
            OversampleRatio::_1024 => 10,
        }
    }

    pub const fn get_cnc_filter_cap(&self) -> u32 {
        2u32.pow(3 * self.log_base_2())
    }
}

/// Third-order sinc filter, with user-specifiable decimation factor
/// The output takes three cycles to settle, so if the iterator
/// doesn't have enough elements to settle, it returns Err(())
#[allow(clippy::result_unit_err)]
pub fn cic_filter_order_3(
    bits: impl IntoIterator<Item = bool>,
    osr: OversampleRatio,
) -> Result<u32, ()> {
    let mod_base = 2 * osr.get_cnc_filter_cap();

    let mut delta_1: u32 = 0;
    let mut cn_1: u32 = 0;
    let mut cn_2: u32 = 0;
    let mut dn_0: u32 = 0;
    let mut dn_1: u32;
    let mut cn_3: u32 = 0;
    let mut dn_3: u32;
    let mut cn_4: u32 = 0;
    let mut dn_5: u32;
    let mut cn_5: u32 = 0;

    let mut bit_iterator = bits.into_iter();

    let samples = osr as usize;

    for _decimation_step in 0..3 {
        for _sample_number in 0..samples {
            let current_bit = match bit_iterator.next() {
                None => return Err(()),
                Some(bit) => bit,
            };
            cn_2 = cn_2.wrapping_add(cn_1) % mod_base;
            cn_1 = cn_1.wrapping_add(delta_1) % mod_base;
            delta_1 = delta_1.wrapping_add(current_bit as u32) % mod_base;
        }

        dn_1 = dn_0;
        dn_3 = cn_3;
        dn_5 = cn_4;

        dn_0 = cn_2;
        cn_3 = dn_0.wrapping_sub(dn_1) % mod_base;
        cn_4 = cn_3.wrapping_sub(dn_3) % mod_base;
        cn_5 = cn_4.wrapping_sub(dn_5) % mod_base;
    }
    Ok(cn_5)
}

#[cfg(test)]
mod test_cic_filter {
    use std::vec;

    use super::{cic_filter_order_3, OversampleRatio};
    use proptest::prelude::*;

    #[test]
    fn test_cic_filter_all_ones() {
        let test_data = vec![true; 50];
        assert_eq!(cic_filter_order_3(test_data, OversampleRatio::_4), Ok(64));
    }

    #[test]
    fn test_cic_filter_all_zeros() {
        let test_data = vec![false; 50];
        assert_eq!(cic_filter_order_3(test_data, OversampleRatio::_4), Ok(0));
    }

    #[test]
    fn test_cic_filter_all_ones_osr_8() {
        let test_data = vec![true; 50];
        assert_eq!(
            cic_filter_order_3(test_data, OversampleRatio::_8),
            Ok(512)
        );
    }

    #[test]
    fn test_cic_filter_alternating_osr_8_true_first() {
        let mut test_data = vec![true; 50];
        for i in 0..25 {
            test_data[2 * i] = false;
        }
        assert_eq!(
            cic_filter_order_3(test_data, OversampleRatio::_8),
            Ok(256)
        );
    }

    #[test]
    fn test_cic_filter_every_fourth_osr_8() {
        let mut test_data = vec![true; 52];
        for i in 0..13 {
            test_data[4 * i + 3] = false;
        }
        assert_eq!(
            cic_filter_order_3(test_data, OversampleRatio::_8),
            Ok(512 * 3 / 4)
        );
    }

    #[test]
    fn test_cic_filter_bunch_of_options() {
        let mut test_data = vec![true; 768];
        for i in 0..(768 / 4) {
            test_data[4 * i] = false;
        }
        assert_eq!(
            cic_filter_order_3(test_data, OversampleRatio::_256),
            Ok(12582912)
        );
    }

    fn simulate_second_order_delta_sigma(input: f64, warmup: usize, num_samples: usize) -> Vec<bool> {
        let mut int1: f64 = 0.0;
        let mut int2: f64 = 0.0;
        let mut feedback: f64 = 0.0;
        let mut output = Vec::with_capacity(num_samples);

        for i in 0..(warmup + num_samples) {
            int1 += input - feedback;
            int2 += int1 - feedback;
            let bit = int2 >= 0.0;
            feedback = if bit { 1.0 } else { 0.0 };
            if i >= warmup {
                output.push(bit);
            }
        }

        output
    }

    proptest! {
        #[test]
        fn test_cic_filter_with_simulated_delta_sigma(input in -0.8_f64..0.8) {
            let osr = OversampleRatio::_256;
            let samples_needed = 256 * 3;
            let warmup = 256;

            let modulator_input = (input + 1.0) / 2.0;

            let bitstream = simulate_second_order_delta_sigma(modulator_input, warmup, samples_needed);
            let result = cic_filter_order_3(bitstream, osr).unwrap();
            let ratio = result as f64 / osr.get_cnc_filter_cap() as f64;
            let error = (ratio - modulator_input).abs();
            let bits_of_accuracy = if error > 0.0 { -error.log2() } else { f64::INFINITY };

            prop_assert!(
                bits_of_accuracy >= 14.0,
                "Input {}: ratio={:.8}, error={:.2e}, ~{:.1} bits (expected >=14)",
                input, ratio, error, bits_of_accuracy
            );
        }
    }
}
