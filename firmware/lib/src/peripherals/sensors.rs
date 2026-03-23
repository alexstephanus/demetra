use core::marker::PhantomData;

use crate::{
    config::calibration::{
        ConductivityCalibration, OrpCalibration,
        OrpMeasurementPoint, PhMeasurementPoint, ThreePointPhCalibration,
    },
    ui_types::SensorType,
    units::{Conductivity, Resistance, Temperature, Voltage},
};

use thiserror::Error;

use log::debug;

#[derive(Error, Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SensorError {
    #[error("Failed to read raw voltage from {0} sensor")]
    HardwareReadFailure(SensorType),
    #[error("{sensor} sensor failed to converge after {attempts} attempts (std dev: {final_std_dev:.2})")]
    ConvergenceFailure {
        sensor: SensorType,
        attempts: usize,
        final_std_dev: f32,
    },
    #[error("{0} sensor calibration data is invalid or missing")]
    CalibrationError(SensorType),
    #[error("Sensor hardware control failure")]
    HardwareControlFailure,
}

const VOLTAGE_VARIANCE_THRESHOLD: f32 = 100.0;
const MAXIMUM_READINGS: usize = 6000;

/// Trait for reading the raw voltage values from various sensors.
/// Intended to encapsulate the circuit-specific mechanics of
/// reading out raw sensor values, while preserving shared functionality
/// in the SensorController: checking for convergence, accounting for
/// existing calibrations, attaching timestamps to read-out data, etc.
#[allow(async_fn_in_trait)]
pub trait SensorReadRaw {
    async fn turn_sensors_on(&mut self) -> Result<(), SensorError>;
    async fn turn_sensors_off(&mut self) -> Result<(), SensorError>;
    async fn read_sensor_raw(&mut self, sensor: SensorType) -> Result<f32, SensorError>;
    fn adc_mv_to_sensor_value(&self, sensor_type: SensorType, raw_adc_mv: f32) -> f32;
}

pub struct SensorController<'a, Sensors> {
    raw_sensors: Sensors,
    voltage_readings: [f32; MAXIMUM_READINGS],
    _phantom: PhantomData<&'a Sensors>,
}

impl<'a, Sensors> SensorController<'a, Sensors>
where
    Sensors: SensorReadRaw,
{
    pub fn new(raw_sensors: Sensors) -> Self {
        Self {
            raw_sensors,
            voltage_readings: [0f32; MAXIMUM_READINGS],
            _phantom: PhantomData,
        }
    }

    pub fn calculate_mean_and_variance(values: &[f32]) -> (f32, f32) {
        let array_len = values.len() as f32;
        let mean = values.into_iter().sum::<f32>() / array_len;
        let mut running_std_dev = 0.0;
        for val in values.iter() {
            running_std_dev += (val - mean) * (val - mean);
        }
        (mean, running_std_dev / array_len)
    }

    pub async fn read_sensor_voltage(&mut self, sensor: SensorType) -> Result<f32, SensorError> {
        for i in 0..MAXIMUM_READINGS {
            self.voltage_readings[i] = self.raw_sensors.read_sensor_raw(sensor).await?;
        }

        let (mean, variance) = Self::calculate_mean_and_variance(&self.voltage_readings);
        debug!("Mean: {:?}, Variance: {:?}", mean, variance);

        if variance < VOLTAGE_VARIANCE_THRESHOLD {
            debug!("Values: {:?}", self.voltage_readings);
            Ok(mean)
        } else {
            debug!("Values: {:?}", self.voltage_readings);
            Err(SensorError::ConvergenceFailure {
                sensor,
                attempts: MAXIMUM_READINGS,
                final_std_dev: variance,
            })
        }
    }

    pub async fn measure_ph(
        &mut self,
        ph_calibration: &ThreePointPhCalibration,
        temperature: Temperature,
    ) -> Result<PhMeasurementPoint, SensorError> {
        debug!("Reading ph");
        let measured_voltage = self.measure_ph_voltage().await?;
        let ph_measurement = ph_calibration.get_calibrated_ph_measurement(temperature, measured_voltage);
        Ok(ph_measurement)
    }

    pub async fn measure_orp(
        &mut self,
        orp_calibration: &OrpCalibration,
    ) -> Result<OrpMeasurementPoint, SensorError> {
        debug!("Reading ORP");
        let adc_voltage = self.read_sensor_voltage(SensorType::Orp).await?;
        let sensor_voltage = self.raw_sensors.adc_mv_to_sensor_value(SensorType::Orp, adc_voltage);
        Ok(orp_calibration.get_calibrated_orp_measurement(Voltage::from_mv(sensor_voltage)))
    }

    pub async fn measure_temperature(&mut self, beta: f32) -> Result<Temperature, SensorError> {
        debug!("Reading temperature");
        let resistance = self.measure_temperature_resistance().await?;
        Ok(Temperature::from_resistance(resistance, beta))
    }

    pub async fn measure_conductivity(
        &mut self,
        conductivity_calibration: &ConductivityCalibration,
        temperature: Temperature,
    ) -> Result<Conductivity, SensorError> {
        debug!("Reading EC");
        let resistance = self.measure_ec_resistance().await?;
        Ok(conductivity_calibration.get_conductivity(resistance, temperature))
    }

    pub async fn turn_sensors_on(&mut self) -> Result<(), SensorError> {
        self.raw_sensors.turn_sensors_on().await
    }

    pub async fn turn_sensors_off(&mut self) -> Result<(), SensorError> {
        self.raw_sensors.turn_sensors_off().await
    }

    pub async fn measure_ph_voltage(&mut self) -> Result<Voltage, SensorError> {
        self.read_sensor_voltage(SensorType::Ph).await.map(Voltage::from_mv)
    }

    pub async fn measure_orp_voltage(&mut self) -> Result<Voltage, SensorError> {
        self.read_sensor_voltage(SensorType::Orp).await.map(Voltage::from_mv)
    }

    pub async fn measure_ec_resistance(&mut self) -> Result<Resistance, SensorError> {
        let voltage = self.read_sensor_voltage(SensorType::Conductivity).await?;
        Ok(Resistance::from_ohms(self.raw_sensors.adc_mv_to_sensor_value(SensorType::Conductivity, voltage)))
    }

    pub async fn measure_temperature_resistance(&mut self) -> Result<Resistance, SensorError> {
        let voltage = self.read_sensor_voltage(SensorType::Temperature).await?;
        Ok(Resistance::from_ohms(self.raw_sensors.adc_mv_to_sensor_value(SensorType::Temperature, voltage)))
    }
}
