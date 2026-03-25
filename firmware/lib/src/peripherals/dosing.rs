use libm::powf;

use crate::{
    config::{
        calibration::{
            NumericRange, OrpMeasurementPoint, PhMeasurementPoint, PhValue, RangePosition,
        },
        device_config::DeviceConfig,
    },
    logging::LoggableError,
    peripherals::{
        Pump, PumpController, PumpError, SensorReadRaw, TreatmentControllerMutex, CURRENT_CUTOFF,
    },
    tasks::SensorReadings,
    units::{Conductivity, Volume},
};

const DOSE_FRACTION: f32 = 0.25;
const ORP_FIXED_DOSE_ML: f32 = 1.0;

pub(crate) async fn run_pump_for_duration<Sensors: SensorReadRaw, Pumps: PumpController>(
    tc: &TreatmentControllerMutex<'_, Sensors, Pumps>,
    pump: &Pump,
    duration: embassy_time::Duration,
) -> Result<(), PumpError> {
    {
        let mut tc = tc.lock().await;
        tc.pump_controller.enable_pump(pump).await?;
        let current = tc.pump_controller.read_current(pump).await?;
        if current < CURRENT_CUTOFF {
            tc.pump_controller.disable_pump(pump).await?;
            return Err(PumpError::NoCurrent);
        }
    }

    embassy_time::Timer::after(duration).await;

    {
        let mut tc = tc.lock().await;
        tc.pump_controller.disable_pump(pump).await?;
        let current = tc.pump_controller.read_current(pump).await?;
        if current >= CURRENT_CUTOFF {
            return Err(PumpError::UnexpectedCurrent);
        }
    }

    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum PrioritizedTreatment {
    None,
    RaiseConductivity(Conductivity),
    RaisePh(PhMeasurementPoint),
    LowerPh(PhMeasurementPoint),
    RaiseOrp(OrpMeasurementPoint),
}

fn has_enabled_pump(pump: Option<&crate::peripherals::DosingPumpState>) -> bool {
    pump.is_some_and(|s| s.enabled)
}

pub fn select_dosing_action(
    config: &DeviceConfig,
    readings: &SensorReadings,
) -> PrioritizedTreatment {
    if let Some(ec) = &readings.ec {
        let pos = NumericRange::new(config.ec.min_acceptable, config.ec.max_acceptable)
            .position(ec.us_per_cm());
        if pos == RangePosition::Low && has_enabled_pump(config.pumps.get_nutrient_pump()) {
            return PrioritizedTreatment::RaiseConductivity(*ec);
        }
    }

    if let Some(ph) = &readings.ph {
        let pos = NumericRange::new(config.ph.min_acceptable, config.ph.max_acceptable)
            .position(ph.ph_value);
        match pos {
            RangePosition::Low if has_enabled_pump(config.pumps.get_ph_up_pump()) => {
                return PrioritizedTreatment::RaisePh(*ph);
            }
            RangePosition::High if has_enabled_pump(config.pumps.get_ph_down_pump()) => {
                return PrioritizedTreatment::LowerPh(*ph);
            }
            _ => {}
        }
    }

    if let Some(orp) = &readings.orp {
        let pos = NumericRange::new(config.orp.min_acceptable, config.orp.max_acceptable)
            .position(orp.voltage.mv());
        if pos == RangePosition::Low && has_enabled_pump(config.pumps.get_orp_pump()) {
            return PrioritizedTreatment::RaiseOrp(*orp);
        }
    }

    PrioritizedTreatment::None
}

/// This is a straightforward stoichiometric calculation.  No accounting for
/// any sort of buffer effects.  So, the effect of any given dose may be
/// overestimated.  Since we measure and dose repeatedly,
/// this is better than potentially trying to account for buffer effects,
/// getting it wrong, and overshooting.
pub(crate) fn calculate_ph_down_dose(
    measured_ph: PhValue,
    target_ph: PhValue,
    tank_size: Volume,
    ph_down_ph: PhValue,
) -> Volume {
    let h_plus_concentration: f32 = powf(10.0, -measured_ph);
    let current_h_plus: f32 = h_plus_concentration * tank_size.to_liters();
    let desired_h_plus = powf(10.0, -target_ph) * tank_size.to_liters();
    let needed_h_plus = desired_h_plus - current_h_plus;

    let solution_h_plus_concentration = powf(10.0, -ph_down_ph);
    Volume::from_liters(needed_h_plus / solution_h_plus_concentration)
}

/// Similarly to calculate_ph_down_dose, this is a stoichiometric calculation
/// liable to undershoot due to buffer effects.
pub(crate) fn calculate_ph_up_dose(
    measured_ph: PhValue,
    target_ph: PhValue,
    tank_size: Volume,
    ph_up_ph: PhValue,
) -> Volume {
    let oh_plus_concentration: f32 = powf(10.0, -14.0 + measured_ph);
    let current_oh_plus: f32 = oh_plus_concentration * tank_size.to_liters();
    let desired_oh_plus = powf(10.0, -14.0 + target_ph) * tank_size.to_liters();
    let needed_oh_plus = desired_oh_plus - current_oh_plus;

    let solution_oh_plus_concentration = powf(10.0, -14.0 + ph_up_ph);
    Volume::from_liters(needed_oh_plus / solution_oh_plus_concentration)
}

pub(crate) fn calculate_ec_dose(
    measured_ec: Conductivity,
    target_ec: Conductivity,
    tank_size: Volume,
    solution_ec: Conductivity,
) -> Volume {
    let deficit = target_ec.us_per_cm() - measured_ec.us_per_cm();
    let dose_liters = deficit * tank_size.to_liters() / solution_ec.us_per_cm();
    Volume::from_liters(dose_liters)
}

pub(crate) async fn stir_and_wait<Sensors: SensorReadRaw, Pumps: PumpController>(
    config: &crate::config::device_config::DeviceConfig,
    tc: &TreatmentControllerMutex<'_, Sensors, Pumps>,
) {
    if let Some(stir_outlet) = config.outlets.get_stir_outlet() {
        let stir_secs = match stir_outlet.stir_seconds {
            Some(s) => s as u64,
            None => {
                log::warn!("Stir outlet has no duration configured, defaulting to 10s");
                10
            }
        };
        let pump = Pump::Cfg(stir_outlet.outlet);
        let duration = embassy_time::Duration::from_secs(stir_secs);
        if let Err(e) = run_pump_for_duration(tc, &pump, duration).await {
            log::error!("Stir pump {:?} error: {:?}", pump, e);
        }
    }
}

pub(crate) async fn dose_ec_step<Sensors: SensorReadRaw, Pumps: PumpController>(
    tc: &TreatmentControllerMutex<'_, Sensors, Pumps>,
    config: &crate::config::device_config::DeviceConfig,
    current_ec: Conductivity,
) -> Result<(), LoggableError> {
    let nutrient_pump_state = match config.pumps.get_nutrient_pump() {
        Some(s) if s.enabled => s,
        _ => return Ok(()),
    };

    let ec_range = NumericRange::new(config.ec.min_acceptable, config.ec.max_acceptable);
    let target_ec = Conductivity::from_us_per_cm(ec_range.midpoint());
    let solution_ec =
        Conductivity::from_us_per_cm(nutrient_pump_state.treatment_solution.solution_strength);

    let total_dose = calculate_ec_dose(current_ec, target_ec, config.tank_size, solution_ec);
    let dose_ml = (total_dose.to_milliliters() * DOSE_FRACTION).max(0.0);

    if dose_ml < 0.05 {
        return Ok(());
    }

    let duration = nutrient_pump_state
        .calibration
        .get_dose_duration(Volume::from_milliliters(dose_ml));
    log::info!(
        "EC dose: {:.2}mL ({}ms), current: {:.0} µS/cm, target: {:.0} µS/cm",
        dose_ml,
        duration.as_millis(),
        current_ec.us_per_cm(),
        ec_range.midpoint()
    );
    run_pump_for_duration(tc, &Pump::Dose(nutrient_pump_state.pump), duration).await?;
    stir_and_wait(config, tc).await;

    Ok(())
}

pub(crate) async fn dose_ph_step<Sensors: SensorReadRaw, Pumps: PumpController>(
    tc: &TreatmentControllerMutex<'_, Sensors, Pumps>,
    config: &crate::config::device_config::DeviceConfig,
    measurement: &PhMeasurementPoint,
    dosing_up: bool,
) -> Result<(), LoggableError> {
    let pump_state = if dosing_up {
        config.pumps.get_ph_up_pump()
    } else {
        config.pumps.get_ph_down_pump()
    };

    let pump_state = match pump_state {
        Some(s) if s.enabled => s,
        _ => return Ok(()),
    };

    let ph_range = NumericRange::new(config.ph.min_acceptable, config.ph.max_acceptable);
    let target_ph = ph_range.midpoint();

    let total_dose = if dosing_up {
        calculate_ph_up_dose(
            measurement.ph_value,
            target_ph,
            config.tank_size,
            pump_state.treatment_solution.solution_strength,
        )
    } else {
        calculate_ph_down_dose(
            measurement.ph_value,
            target_ph,
            config.tank_size,
            pump_state.treatment_solution.solution_strength,
        )
    };

    let dose_ml = (total_dose.to_milliliters() * DOSE_FRACTION).max(0.0);

    if dose_ml < 0.05 {
        return Ok(());
    }

    let direction = if dosing_up { "up" } else { "down" };
    let duration = pump_state
        .calibration
        .get_dose_duration(Volume::from_milliliters(dose_ml));
    log::info!(
        "pH {} dose: {:.2}mL ({}ms), current: {:.2}, target: {:.2}",
        direction,
        dose_ml,
        duration.as_millis(),
        measurement.ph_value,
        target_ph
    );
    run_pump_for_duration(tc, &Pump::Dose(pump_state.pump), duration).await?;
    stir_and_wait(config, tc).await;

    Ok(())
}

pub(crate) async fn dose_orp_step<Sensors: SensorReadRaw, Pumps: PumpController>(
    tc: &TreatmentControllerMutex<'_, Sensors, Pumps>,
    config: &crate::config::device_config::DeviceConfig,
    measurement: &OrpMeasurementPoint,
) -> Result<(), LoggableError> {
    let orp_pump_state = match config.pumps.get_orp_pump() {
        Some(s) if s.enabled => s,
        _ => return Ok(()),
    };

    let dose_volume = Volume::from_milliliters(ORP_FIXED_DOSE_ML);
    let duration = orp_pump_state.calibration.get_dose_duration(dose_volume);
    log::info!(
        "ORP dose: {:.1}mL ({}ms), current: {:.0}mV",
        ORP_FIXED_DOSE_ML,
        duration.as_millis(),
        measurement.voltage.mv()
    );
    run_pump_for_duration(tc, &Pump::Dose(orp_pump_state.pump), duration).await?;
    stir_and_wait(config, tc).await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_types::TreatmentSolutionType;

    #[test]
    fn ph_up_computes_proper_volume() {
        let dose = calculate_ph_up_dose(6.0, 7.0, Volume::from_liters(10.0), 10.0);
        let expected_ml = 9.0;
        let actual_ml = dose.to_milliliters();
        assert!(
            (actual_ml - expected_ml).abs() < 0.01,
            "Expected {:.1} mL, got {:.2} mL",
            expected_ml,
            actual_ml
        );
    }

    #[test]
    fn ph_down_computes_proper_volume() {
        let dose = calculate_ph_down_dose(10.0, 9.0, Volume::from_liters(10.0), 6.0);
        let expected_ml = 9.0;
        let actual_ml = dose.to_milliliters();
        assert!(
            (actual_ml - expected_ml).abs() < 0.01,
            "Expected {:.1} mL, got {:.1} mL",
            expected_ml,
            actual_ml
        );
    }

    #[test]
    fn ph_up_dose_proportional_to_tank_size() {
        let dose_10l = calculate_ph_up_dose(6.5, 7.0, Volume::from_liters(10.0), 10.0);
        let dose_20l = calculate_ph_up_dose(6.5, 7.0, Volume::from_liters(20.0), 10.0);
        let ratio = dose_20l.to_milliliters() / dose_10l.to_milliliters();
        assert!(
            (ratio - 2.0).abs() < 0.001,
            "Expected 2x dose for 2x tank, got {:.4}x",
            ratio
        );
    }

    #[test]
    fn ph_up_dose_zero_when_at_target() {
        let dose = calculate_ph_up_dose(7.0, 7.0, Volume::from_liters(100.0), 10.0);
        assert!(
            dose.to_milliliters().abs() < 0.0001,
            "Expected ~0 mL, got {:.6} mL",
            dose.to_milliliters()
        );
    }

    #[test]
    fn ph_up_dose_stronger_solution_gives_appropriately_smaller_dose() {
        let dose_ph10 = calculate_ph_up_dose(6.5, 7.0, Volume::from_liters(10.0), 10.0);
        let dose_ph11 = calculate_ph_up_dose(6.5, 7.0, Volume::from_liters(10.0), 11.0);
        let ratio = dose_ph10.to_milliliters() / dose_ph11.to_milliliters();
        assert!(
            (ratio - 10.0).abs() < 0.01,
            "Expected 10x dose for 1 pH unit weaker solution, got {:.4}x",
            ratio
        );
    }

    #[test]
    fn ph_up_dose_positive_when_below_range() {
        let dose = calculate_ph_up_dose(6.0, 7.0, Volume::from_liters(50.0), 10.0);
        assert!(
            dose.to_milliliters() > 0.0,
            "Expected positive dose when pH is below range"
        );
    }

    #[test]
    fn ph_down_dose_proportional_to_tank_size() {
        let dose_10l = calculate_ph_down_dose(7.5, 7.0, Volume::from_liters(10.0), 3.0);
        let dose_20l = calculate_ph_down_dose(7.5, 7.0, Volume::from_liters(20.0), 3.0);
        let ratio = dose_20l.to_milliliters() / dose_10l.to_milliliters();
        assert!(
            (ratio - 2.0).abs() < 0.001,
            "Expected 2x dose for 2x tank, got {:.4}x",
            ratio
        );
    }

    #[test]
    fn ph_down_dose_zero_when_at_target() {
        let dose = calculate_ph_down_dose(7.0, 7.0, Volume::from_liters(100.0), 3.0);
        assert!(
            dose.to_milliliters().abs() < 0.0001,
            "Expected ~0 mL, got {:.6} mL",
            dose.to_milliliters()
        );
    }

    #[test]
    fn ph_down_dose_stronger_solution_gives_smaller_dose() {
        let dose_ph3 = calculate_ph_down_dose(7.5, 7.0, Volume::from_liters(10.0), 3.0);
        let dose_ph2 = calculate_ph_down_dose(7.5, 7.0, Volume::from_liters(10.0), 2.0);
        let ratio = dose_ph3.to_milliliters() / dose_ph2.to_milliliters();
        assert!(
            (ratio - 10.0).abs() < 0.01,
            "Expected 10x dose for 1 pH unit weaker solution, got {:.4}x",
            ratio
        );
    }

    #[test]
    fn ph_down_dose_positive_when_above_range() {
        let dose = calculate_ph_down_dose(8.0, 7.0, Volume::from_liters(50.0), 3.0);
        assert!(
            dose.to_milliliters() > 0.0,
            "Expected positive dose when pH is above range"
        );
    }

    fn ec(us: f32) -> Conductivity {
        Conductivity::from_us_per_cm(us)
    }

    #[test]
    fn ec_dose_proportional_to_tank_size() {
        let dose_10l = calculate_ec_dose(
            ec(500.0),
            ec(800.0),
            Volume::from_liters(10.0),
            ec(50_000.0),
        );
        let dose_20l = calculate_ec_dose(
            ec(500.0),
            ec(800.0),
            Volume::from_liters(20.0),
            ec(50_000.0),
        );
        let ratio = dose_20l.to_milliliters() / dose_10l.to_milliliters();
        assert!(
            (ratio - 2.0).abs() < 0.001,
            "Expected 2x dose for 2x tank, got {:.4}x",
            ratio
        );
    }

    #[test]
    fn ec_dose_zero_when_at_target() {
        let dose = calculate_ec_dose(
            ec(800.0),
            ec(800.0),
            Volume::from_liters(10.0),
            ec(50_000.0),
        );
        assert!(
            dose.to_milliliters().abs() < 0.0001,
            "Expected ~0 mL, got {:.6} mL",
            dose.to_milliliters()
        );
    }

    #[test]
    fn ec_dose_larger_deficit_gives_larger_dose() {
        let small_deficit = calculate_ec_dose(
            ec(700.0),
            ec(800.0),
            Volume::from_liters(50.0),
            ec(50_000.0),
        );
        let large_deficit = calculate_ec_dose(
            ec(400.0),
            ec(800.0),
            Volume::from_liters(50.0),
            ec(50_000.0),
        );
        assert!(
            large_deficit.to_milliliters() > small_deficit.to_milliliters(),
            "Larger EC deficit should give larger dose"
        );
    }

    #[test]
    fn ec_dose_stronger_solution_gives_smaller_dose() {
        let weak = calculate_ec_dose(
            ec(500.0),
            ec(800.0),
            Volume::from_liters(10.0),
            ec(25_000.0),
        );
        let strong = calculate_ec_dose(
            ec(500.0),
            ec(800.0),
            Volume::from_liters(10.0),
            ec(50_000.0),
        );
        let ratio = weak.to_milliliters() / strong.to_milliliters();
        assert!(
            (ratio - 2.0).abs() < 0.001,
            "Expected 2x dose for half-strength solution, got {:.4}x",
            ratio
        );
    }

    fn enable_pump(
        config: &mut DeviceConfig,
        pump: crate::ui_types::DosingPump,
        solution_type: TreatmentSolutionType,
    ) {
        let state = config.pumps.get_dosing_pump_state_mut(pump);
        state.enabled = true;
        state.treatment_solution.solution_type = solution_type;
        state.treatment_solution.solution_strength = 1000.0;
    }

    fn config_with_ranges(ec: (f32, f32), ph: (f32, f32), orp: (f32, f32)) -> DeviceConfig {
        let mut config = DeviceConfig::default();
        config.ec.min_acceptable = ec.0;
        config.ec.max_acceptable = ec.1;
        config.ph.min_acceptable = ph.0;
        config.ph.max_acceptable = ph.1;
        config.orp.min_acceptable = orp.0;
        config.orp.max_acceptable = orp.1;
        enable_pump(
            &mut config,
            crate::ui_types::DosingPump::DoseOne,
            TreatmentSolutionType::Nutrient,
        );
        enable_pump(
            &mut config,
            crate::ui_types::DosingPump::DoseTwo,
            TreatmentSolutionType::PhUp,
        );
        enable_pump(
            &mut config,
            crate::ui_types::DosingPump::DoseThree,
            TreatmentSolutionType::PhDown,
        );
        enable_pump(
            &mut config,
            crate::ui_types::DosingPump::DoseFour,
            TreatmentSolutionType::OrpTreatment,
        );
        config
    }

    use crate::units::{Temperature, Voltage};

    fn ph(value: f32) -> PhMeasurementPoint {
        PhMeasurementPoint::new(value, Voltage::from_mv(0.0), Temperature::default())
    }

    fn readings(ec_val: Option<f32>, ph_val: Option<f32>, orp_val: Option<f32>) -> SensorReadings {
        SensorReadings {
            temperature: None,
            ec: ec_val.map(Conductivity::from_us_per_cm),
            ph: ph_val.map(ph),
            orp: orp_val.map(|v| OrpMeasurementPoint {
                voltage: Voltage::from_mv(v),
            }),
        }
    }

    #[test]
    fn select_none_when_all_settled() {
        let config = config_with_ranges((500.0, 1000.0), (6.0, 7.5), (400.0, 800.0));
        assert_eq!(
            select_dosing_action(&config, &readings(Some(750.0), Some(6.75), Some(600.0))),
            PrioritizedTreatment::None,
        );
    }

    #[test]
    fn select_none_when_no_readings() {
        let config = config_with_ranges((500.0, 1000.0), (6.0, 7.5), (400.0, 800.0));
        assert_eq!(
            select_dosing_action(&config, &readings(None, None, None)),
            PrioritizedTreatment::None,
        );
    }

    #[test]
    fn select_ec_over_ph_and_orp() {
        let config = config_with_ranges((500.0, 1000.0), (6.0, 7.5), (400.0, 800.0));
        assert!(matches!(
            select_dosing_action(&config, &readings(Some(400.0), Some(5.0), Some(300.0))),
            PrioritizedTreatment::RaiseConductivity(_),
        ));
    }

    #[test]
    fn select_ph_when_ec_settled() {
        let config = config_with_ranges((500.0, 1000.0), (6.0, 7.5), (400.0, 800.0));
        assert!(matches!(
            select_dosing_action(&config, &readings(Some(750.0), Some(5.0), Some(300.0))),
            PrioritizedTreatment::RaisePh(_),
        ));
    }

    #[test]
    fn select_lower_ph_when_high() {
        let config = config_with_ranges((500.0, 1000.0), (6.0, 7.5), (400.0, 800.0));
        assert!(matches!(
            select_dosing_action(&config, &readings(Some(750.0), Some(8.0), Some(300.0))),
            PrioritizedTreatment::LowerPh(_),
        ));
    }

    #[test]
    fn select_orp_when_ec_and_ph_settled() {
        let config = config_with_ranges((500.0, 1000.0), (6.0, 7.5), (400.0, 800.0));
        assert!(matches!(
            select_dosing_action(&config, &readings(Some(750.0), Some(6.75), Some(300.0))),
            PrioritizedTreatment::RaiseOrp(_),
        ));
    }

    #[test]
    fn select_skips_missing_readings() {
        let config = config_with_ranges((500.0, 1000.0), (6.0, 7.5), (400.0, 800.0));
        assert!(matches!(
            select_dosing_action(&config, &readings(None, Some(5.0), Some(300.0))),
            PrioritizedTreatment::RaisePh(_),
        ));
        assert!(matches!(
            select_dosing_action(&config, &readings(None, None, Some(300.0))),
            PrioritizedTreatment::RaiseOrp(_),
        ));
    }

    #[test]
    fn select_uses_inner_band_not_outer_range() {
        let config = config_with_ranges((500.0, 1000.0), (6.0, 8.0), (400.0, 800.0));
        assert!(matches!(
            select_dosing_action(&config, &readings(None, Some(6.3), None)),
            PrioritizedTreatment::RaisePh(_),
        ));
        assert!(matches!(
            select_dosing_action(&config, &readings(None, Some(7.7), None)),
            PrioritizedTreatment::LowerPh(_),
        ));
        assert_eq!(
            select_dosing_action(&config, &readings(None, Some(7.0), None)),
            PrioritizedTreatment::None,
        );
    }

    fn config_with_ranges_no_pumps(
        ec: (f32, f32),
        ph: (f32, f32),
        orp: (f32, f32),
    ) -> DeviceConfig {
        let mut config = DeviceConfig::default();
        config.ec.min_acceptable = ec.0;
        config.ec.max_acceptable = ec.1;
        config.ph.min_acceptable = ph.0;
        config.ph.max_acceptable = ph.1;
        config.orp.min_acceptable = orp.0;
        config.orp.max_acceptable = orp.1;
        config
    }

    #[test]
    fn select_none_when_no_pumps_configured() {
        let config = config_with_ranges_no_pumps((500.0, 1000.0), (6.0, 7.5), (400.0, 800.0));
        assert_eq!(
            select_dosing_action(&config, &readings(Some(400.0), Some(5.0), Some(300.0))),
            PrioritizedTreatment::None,
        );
    }

    #[test]
    fn select_falls_through_to_ph_when_no_nutrient_pump() {
        let mut config = config_with_ranges_no_pumps((500.0, 1000.0), (6.0, 7.5), (400.0, 800.0));
        enable_pump(
            &mut config,
            crate::ui_types::DosingPump::DoseTwo,
            TreatmentSolutionType::PhUp,
        );
        assert!(matches!(
            select_dosing_action(&config, &readings(Some(400.0), Some(5.0), Some(300.0))),
            PrioritizedTreatment::RaisePh(_),
        ));
    }

    #[test]
    fn select_falls_through_to_orp_when_no_ec_or_ph_pumps() {
        let mut config = config_with_ranges_no_pumps((500.0, 1000.0), (6.0, 7.5), (400.0, 800.0));
        enable_pump(
            &mut config,
            crate::ui_types::DosingPump::DoseFour,
            TreatmentSolutionType::OrpTreatment,
        );
        assert!(matches!(
            select_dosing_action(&config, &readings(Some(400.0), Some(5.0), Some(300.0))),
            PrioritizedTreatment::RaiseOrp(_),
        ));
    }

    #[test]
    fn select_skips_disabled_pump() {
        let mut config = config_with_ranges_no_pumps((500.0, 1000.0), (6.0, 7.5), (400.0, 800.0));
        let state = config
            .pumps
            .get_dosing_pump_state_mut(crate::ui_types::DosingPump::DoseOne);
        state.treatment_solution.solution_type = TreatmentSolutionType::Nutrient;
        state.enabled = false;
        enable_pump(
            &mut config,
            crate::ui_types::DosingPump::DoseTwo,
            TreatmentSolutionType::PhUp,
        );
        assert!(matches!(
            select_dosing_action(&config, &readings(Some(400.0), Some(5.0), None)),
            PrioritizedTreatment::RaisePh(_),
        ));
    }

    #[test]
    fn select_falls_through_ph_down_when_no_ph_down_pump() {
        let mut config = config_with_ranges_no_pumps((500.0, 1000.0), (6.0, 7.5), (400.0, 800.0));
        enable_pump(
            &mut config,
            crate::ui_types::DosingPump::DoseFour,
            TreatmentSolutionType::OrpTreatment,
        );
        assert!(matches!(
            select_dosing_action(&config, &readings(Some(750.0), Some(8.0), Some(300.0))),
            PrioritizedTreatment::RaiseOrp(_),
        ));
    }
}
