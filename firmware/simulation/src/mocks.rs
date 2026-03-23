use std::io::{self, Write};
use anyhow::Result;
use tokio::sync::Mutex;
use std::sync::Arc;
use lib::peripherals::{SensorReadRaw, SensorError, PumpController, PumpError, DosingPump, Pump};
use lib::ui_types::Outlet;
use lib::peripherals::rtc::{RealTimeClock, RtcError};
use lib::ui_types::SensorType;

use chrono::{DateTime, Utc};

/// Mock sensor implementations that prompt for CLI input
#[derive(Debug, Clone)]
pub struct CliSensors {
    pub ph_sensor: Arc<Mutex<CliPhSensor>>,
    pub ec_sensor: Arc<Mutex<CliEcSensor>>, 
    pub orp_sensor: Arc<Mutex<CliOrpSensor>>,
    pub temperature_sensor: Arc<Mutex<CliTemperatureSensor>>,
}

impl CliSensors {
    pub fn new() -> Self {
        Self {
            ph_sensor: Arc::new(Mutex::new(CliPhSensor::new())),
            ec_sensor: Arc::new(Mutex::new(CliEcSensor::new())),
            orp_sensor: Arc::new(Mutex::new(CliOrpSensor::new())),
            temperature_sensor: Arc::new(Mutex::new(CliTemperatureSensor::new())),
        }
    }
}

impl SensorReadRaw for CliSensors {
    async fn turn_sensors_on(&mut self) -> Result<(), SensorError> {
        println!("Mock sensors turned ON");
        Ok(())
    }

    async fn turn_sensors_off(&mut self) -> Result<(), SensorError> {
        println!("Mock sensors turned OFF");
        Ok(())
    }

    async fn read_sensor_raw(&mut self, sensor: SensorType) -> Result<f32, SensorError> {
        println!("CliSensors::read_sensor_raw called for sensor: {:?}", sensor);
        match sensor {
            SensorType::Ph => {
                println!("Reading pH sensor...");
                let ph_sensor = self.ph_sensor.lock().await;
                ph_sensor.read_raw_voltage().await.map_err(|_| SensorError::HardwareReadFailure(sensor))
            }
            SensorType::Conductivity => {
                println!("Reading conductivity sensor...");
                let ec_sensor = self.ec_sensor.lock().await;
                ec_sensor.read_raw_voltage().await.map_err(|_| SensorError::HardwareReadFailure(sensor))
            }
            SensorType::Orp => {
                println!("Reading ORP sensor...");
                let orp_sensor = self.orp_sensor.lock().await;
                orp_sensor.read_raw_voltage().await.map_err(|_| SensorError::HardwareReadFailure(sensor))
            }
            SensorType::Temperature => {
                println!("Reading temperature sensor...");
                let temp_sensor = self.temperature_sensor.lock().await;
                temp_sensor.read_temperature().await.map_err(|_| SensorError::HardwareReadFailure(sensor))
            }
        }
    }

    fn adc_mv_to_sensor_value(&self, _sensor_type: SensorType, raw_adc_mv: f32) -> f32 {
        raw_adc_mv
    }
}

#[derive(Debug)]
pub struct CliPhSensor;

impl CliPhSensor {
    pub fn new() -> Self {
        Self
    }
    
    /// Prompt for pH measurement voltage (what the ADC would read)
    pub async fn read_raw_voltage(&self) -> Result<f32> {
        prompt_for_input("pH measurement voltage (0.0-3.3V)").await
    }
}

#[derive(Debug)]
pub struct CliEcSensor;

impl CliEcSensor {
    pub fn new() -> Self {
        Self
    }
    
    /// Prompt for EC measurement voltage
    pub async fn read_raw_voltage(&self) -> Result<f32> {
        prompt_for_input("conductivity measurement voltage (0.0-3.3V)").await
    }
}

#[derive(Debug)]
pub struct CliOrpSensor;

impl CliOrpSensor {
    pub fn new() -> Self {
        Self
    }
    
    /// Prompt for ORP measurement voltage
    pub async fn read_raw_voltage(&self) -> Result<f32> {
        prompt_for_input("ORP measurement voltage (0.0-3.3V)").await
    }
}

#[derive(Debug)]
pub struct CliTemperatureSensor;

impl CliTemperatureSensor {
    pub fn new() -> Self {
        Self
    }
    
    /// Prompt for temperature directly (temperature sensor doesn't use voltage)
    pub async fn read_temperature(&self) -> Result<f32> {
        prompt_for_input("temperature (°C)").await
    }
}

#[derive(Debug)]
pub struct MockRtc {
    // Offset to add to system time to get the "RTC" time
    time_offset: std::sync::RwLock<chrono::TimeDelta>,
}

impl MockRtc {
    pub fn new() -> Self {
        Self {
            time_offset: std::sync::RwLock::new(chrono::TimeDelta::zero()),
        }
    }
}

impl RealTimeClock for MockRtc {
    async fn get_datetime(&mut self) -> Result<DateTime<Utc>, RtcError> {
        let offset = *self.time_offset.read().unwrap();
        let rtc_time = Utc::now() + offset;
        println!("MockRTC: Getting datetime = {}", rtc_time);
        Ok(rtc_time)
    }

    async fn set_datetime(&mut self, datetime: DateTime<Utc>) -> Result<(), RtcError> {
        let now = Utc::now();
        let new_offset = datetime - now;
        *self.time_offset.write().unwrap() = new_offset;
        println!("MockRTC: Set datetime to {} (offset: {})", datetime, new_offset);
        Ok(())
    }
}


#[derive(Debug)]
pub struct CliCurrentSenseAdc;

impl CliCurrentSenseAdc {
    pub fn new() -> Self {
        Self
    }
    
    pub async fn read_current(&self, channel: u8) -> Result<f32> {
        let result = prompt_for_input(&format!("current sense ADC channel {} (mA)", channel)).await?;
        Ok(result)
    }
}

/// Helper function to prompt for CLI input with proper formatting
async fn prompt_for_input(prompt: &str) -> Result<f32> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    
    // Print prompt
    print!("\nInput {}: ", prompt);
    io::stdout().flush().map_err(|e| anyhow::anyhow!("Failed to flush stdout: {}", e))?;
    
    // Read input from stdin
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut input = String::new();
    
    reader.read_line(&mut input).await
        .map_err(|e| anyhow::anyhow!("Failed to read input: {}", e))?;
    
    // Parse the input
    let value: f32 = input.trim().parse()
        .map_err(|e| anyhow::anyhow!("Invalid number '{}': {}", input.trim(), e))?;
    
    println!("Received: {}", value);
    Ok(value)
}

/// Mock pump controller that prints operations instead of controlling hardware
#[derive(Debug)]
pub struct CliPumpController {
    dosing_pump_states: [bool; 6],
    outlet_states: [bool; 4],
}

impl CliPumpController {
    pub fn new() -> Self {
        Self {
            dosing_pump_states: [false; 6],
            outlet_states: [false; 4],
        }
    }
}

impl PumpController for CliPumpController {
    async fn enable_pump(&mut self, pump: &Pump) -> Result<(), PumpError> {
        println!("MockPumpController: Enabling pump {:?}", pump);
        match pump {
            Pump::Dose(d) => self.dosing_pump_states[d.to_int()] = true,
            Pump::Cfg(o) => self.outlet_states[o.to_int()] = true,
        }
        lib::ui_backend::state::set_pump_active(pump, true).await;
        Ok(())
    }

    async fn disable_pump(&mut self, pump: &Pump) -> Result<(), PumpError> {
        println!("MockPumpController: Disabling pump {:?}", pump);
        match pump {
            Pump::Dose(d) => self.dosing_pump_states[d.to_int()] = false,
            Pump::Cfg(o) => self.outlet_states[o.to_int()] = false,
        }
        lib::ui_backend::state::set_pump_active(pump, false).await;
        Ok(())
    }

    async fn read_current(&mut self, pump: &Pump) -> Result<f32, PumpError> {
        let enabled = self.is_pump_enabled(pump)?;
        let current = if !enabled {
            0.0
        } else {
            match pump {
                Pump::Dose(dosing_pump) => match dosing_pump {
                    DosingPump::DoseOne => 0.15,
                    DosingPump::DoseTwo => 0.18,
                    DosingPump::DoseThree => 0.16,
                    DosingPump::DoseFour => 0.14,
                    DosingPump::DoseFive => 0.17,
                    DosingPump::DoseSix => 0.19,
                },
                Pump::Cfg(outlet) => match outlet {
                    Outlet::One => 0.25,
                    Outlet::Two => 0.30,
                    Outlet::Three => 0.28,
                    Outlet::Four => 0.27,
                },
            }
        };
        println!("MockPumpController: Reading current for {:?} = {}A (enabled: {})", pump, current, enabled);
        Ok(current)
    }

    async fn turn_off_all(&mut self) -> Result<(), PumpError> {
        println!("MockPumpController: Turning OFF all pumps and outlets");
        self.dosing_pump_states = [false; 6];
        self.outlet_states = [false; 4];
        lib::ui_backend::state::clear_all_pump_states().await;
        Ok(())
    }

    fn is_pump_enabled(&mut self, pump: &Pump) -> Result<bool, PumpError> {
        match pump {
            Pump::Dose(d) => Ok(self.dosing_pump_states[d.to_int()]),
            Pump::Cfg(o) => Ok(self.outlet_states[o.to_int()]),
        }
    }

    fn enable_relay(&mut self) {
        println!("MockPumpController: Relay enabled");
    }

    fn kill_relay(&mut self) {
        println!("MockPumpController: KILLING RELAY - emergency power cut");
        self.dosing_pump_states = [false; 6];
        self.outlet_states = [false; 4];
    }
}