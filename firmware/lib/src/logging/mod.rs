pub mod storage;

cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::string::String;
        use std::vec::Vec;
        use std::format;
    } else {
        use alloc::string::String;
        use alloc::vec::Vec;
        use alloc::format;
    }
}

use chrono::{DateTime, Utc};
use core::fmt::Debug;
use serde::{Deserialize, Serialize};

use crate::peripherals::{PumpError, SensorError};

#[derive(thiserror::Error, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum LoggableError {
    #[error(transparent)]
    Pump(#[from] PumpError),
    #[error(transparent)]
    Sensor(#[from] SensorError),
    #[error("{0}")]
    Hardware(String),
}

impl LoggableError {
    pub fn log_category(&self) -> LogCategory {
        match self {
            LoggableError::Pump(_) => LogCategory::Pump,
            LoggableError::Sensor(_) => LogCategory::Sensor,
            LoggableError::Hardware(_) => LogCategory::Hardware,
        }
    }

    pub fn error_type(&self) -> ErrorType {
        ErrorType::Hardware
    }
}

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};

use crate::config::calibration::types::TimestampedValue;
use embedded_storage::Storage;

pub use storage::{ErrorType, LogCategory, LogLevel, LogMetadata, LogRingBuffer};

pub type StructuredContext = serde_json::Map<String, serde_json::Value>;

pub struct LogRequest {
    pub message: String,
    pub micros: u64,
    pub level: LogLevel,
    pub category: LogCategory,
    pub error_type: Option<ErrorType>,
    pub context: Option<StructuredContext>,
}

pub const LOG_CHANNEL_SIZE: usize = 16;
pub static LOG_CHANNEL: Channel<CriticalSectionRawMutex, LogRequest, LOG_CHANNEL_SIZE> = Channel::new();

pub fn flash_log_error(error: &LoggableError) {
    let message = format!("{}", error);
    log::error!("{}", message);
    if LOG_CHANNEL.try_send(LogRequest {
        message,
        micros: crate::state::current_micros(),
        level: LogLevel::Error,
        category: error.log_category(),
        error_type: Some(error.error_type()),
        context: None,
    }).is_err() {
        log::warn!("Log channel full, flash error entry dropped");
    }
}

pub fn flash_log_warn(message: String, category: LogCategory) {
    log::warn!("{}", message);
    if LOG_CHANNEL.try_send(LogRequest {
        message,
        micros: crate::state::current_micros(),
        level: LogLevel::Warn,
        category,
        error_type: None,
        context: None,
    }).is_err() {
        log::warn!("Log channel full, flash warn entry dropped");
    }
}

pub fn flash_log_info(message: String, category: LogCategory) {
    log::info!("{}", message);
    if LOG_CHANNEL.try_send(LogRequest {
        message,
        micros: crate::state::current_micros(),
        level: LogLevel::Info,
        category,
        error_type: None,
        context: None,
    }).is_err() {
        log::warn!("Log channel full, flash info entry dropped");
    }
}

pub fn flash_log_sensor_readings(readings: &crate::tasks::SensorReadings) {
    use crate::ui_types::SensorType;

    let mut context = serde_json::Map::new();
    let mut parts: Vec<String> = Vec::new();

    let values: [(&str, SensorType, Option<f32>); 4] = [
        ("temperature", SensorType::Temperature, readings.temperature.map(|t| t.celsius())),
        ("ec", SensorType::Conductivity, readings.ec.map(|c| c.us_per_cm())),
        ("ph", SensorType::Ph, readings.ph.as_ref().map(|p| p.ph_value)),
        ("orp", SensorType::Orp, readings.orp.as_ref().map(|o| o.voltage.mv())),
    ];

    let micros = crate::state::current_micros();
    let timestamp_secs = micros as i64 / 1_000_000;

    for (name, sensor_type, value) in &values {
        if let Some(v) = value {
            parts.push(format!("{}: {:.2}", name, v));
            if let Some(num) = serde_json::Number::from_f64(*v as f64) {
                context.insert(String::from(*name), serde_json::Value::Number(num));
            }
            crate::ui_backend::state::push_sensor_reading(*sensor_type, timestamp_secs, *v);
        }
    }

    if parts.is_empty() {
        return;
    }

    let message = parts.join(", ");
    log::info!("Sensors: {}", message);

    if LOG_CHANNEL.try_send(LogRequest {
        message,
        micros,
        level: LogLevel::Info,
        category: LogCategory::Sensor,
        error_type: None,
        context: Some(context),
    }).is_err() {
        log::warn!("Log channel full, sensor readings entry dropped");
    }
}

pub fn process_log_request<L: Logger>(request: LogRequest, logger: &mut L) {
    let level = request.level;
    let category = request.category;
    let timestamp_secs = request.micros as i64 / 1_000_000;
    let error_message = if level == LogLevel::Error {
        Some(request.message.clone())
    } else {
        None
    };
    let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp_secs, 0)
        .unwrap_or_default();
    let entry = match request.context {
        Some(ctx) => LogEntry::with_context(request.message, timestamp, ctx),
        None => LogEntry::new(request.message, timestamp),
    };
    if logger.log(entry, level, category, request.error_type).is_err() {
        log::error!("Failed to write log entry");
    }
    if let Some(msg) = error_message {
        crate::ui_backend::state::push_recent_error(crate::ui_backend::state::RecentError {
            message: msg,
            category,
            timestamp_secs,
        });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub context: Option<StructuredContext>,
}

#[cfg(test)]
impl Eq for LogEntry {}

impl TimestampedValue for LogEntry {
    fn get_written_timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

impl LogEntry {
    pub fn new(message: String, timestamp: DateTime<Utc>) -> Self {
        Self {
            message,
            timestamp,
            context: None,
        }
    }

    pub fn with_context(
        message: String,
        timestamp: DateTime<Utc>,
        context: StructuredContext,
    ) -> Self {
        Self {
            message,
            timestamp,
            context: Some(context),
        }
    }

    pub fn add_context_value<T: Into<serde_json::Value>>(&mut self, key: String, value: T) {
        let context = self.context.get_or_insert_with(serde_json::Map::new);
        context.insert(key, value.into());
    }
}

pub trait Logger {
    type Error;

    fn log(
        &mut self,
        entry: LogEntry,
        level: LogLevel,
        category: LogCategory,
        error_type: Option<ErrorType>,
    ) -> Result<(), Self::Error>;

    fn info(
        &mut self,
        message: String,
        timestamp: DateTime<Utc>,
        category: LogCategory,
    ) -> Result<(), Self::Error> {
        let entry = LogEntry::new(message, timestamp);
        self.log(entry, LogLevel::Info, category, None)
    }

    fn warn(
        &mut self,
        message: String,
        timestamp: DateTime<Utc>,
        category: LogCategory,
    ) -> Result<(), Self::Error> {
        let entry = LogEntry::new(message, timestamp);
        self.log(entry, LogLevel::Warn, category, None)
    }

    fn error(
        &mut self,
        message: String,
        timestamp: DateTime<Utc>,
        category: LogCategory,
        error_type: ErrorType,
    ) -> Result<(), Self::Error> {
        let entry = LogEntry::new(message, timestamp);
        self.log(entry, LogLevel::Error, category, Some(error_type))
    }

    fn debug(
        &mut self,
        message: String,
        timestamp: DateTime<Utc>,
        category: LogCategory,
    ) -> Result<(), Self::Error> {
        let entry = LogEntry::new(message, timestamp);
        self.log(entry, LogLevel::Debug, category, None)
    }

    fn info_with_context(
        &mut self,
        message: String,
        timestamp: DateTime<Utc>,
        context: StructuredContext,
        category: LogCategory,
    ) -> Result<(), Self::Error> {
        let entry = LogEntry::with_context(message, timestamp, context);
        self.log(entry, LogLevel::Info, category, None)
    }

    fn warn_with_context(
        &mut self,
        message: String,
        timestamp: DateTime<Utc>,
        context: StructuredContext,
        category: LogCategory,
    ) -> Result<(), Self::Error> {
        let entry = LogEntry::with_context(message, timestamp, context);
        self.log(entry, LogLevel::Warn, category, None)
    }

    fn error_with_context(
        &mut self,
        message: String,
        timestamp: DateTime<Utc>,
        context: StructuredContext,
        category: LogCategory,
        error_type: ErrorType,
    ) -> Result<(), Self::Error> {
        let entry = LogEntry::with_context(message, timestamp, context);
        self.log(entry, LogLevel::Error, category, Some(error_type))
    }

    fn debug_with_context(
        &mut self,
        message: String,
        timestamp: DateTime<Utc>,
        context: StructuredContext,
        category: LogCategory,
    ) -> Result<(), Self::Error> {
        let entry = LogEntry::with_context(message, timestamp, context);
        self.log(entry, LogLevel::Debug, category, None)
    }
}

pub struct RingBufferLogger<S, E> {
    ring_buffer: LogRingBuffer<S, E>,
    log_level: LogLevel,
}

impl<S, E> RingBufferLogger<S, E> {
    pub fn new(ring_buffer: LogRingBuffer<S, E>) -> Self {
        Self {
            ring_buffer,
            log_level: LogLevel::Info,
        }
    }

    pub fn with_log_level(ring_buffer: LogRingBuffer<S, E>, log_level: LogLevel) -> Self {
        Self {
            ring_buffer,
            log_level,
        }
    }

    pub fn set_log_level(&mut self, log_level: LogLevel) {
        self.log_level = log_level;
    }

    pub fn get_log_level(&self) -> LogLevel {
        self.log_level
    }

    fn should_log(&self, level: LogLevel) -> bool {
        (level as u8) <= (self.log_level as u8)
    }
}

impl<S, E> RingBufferLogger<S, E>
where
    S: Storage<Error = E>,
    E: Debug,
{
    pub fn get_sensor_readings(&mut self, sensor_name: &str) -> Vec<(DateTime<Utc>, f32)> {
        let filter = LogFilter::new()
            .level(LogLevel::Info)
            .category(LogCategory::Sensor);

        let mut readings = Vec::new();
        let filtered_iter = self
            .ring_buffer
            .iter_filtered(|metadata| filter.matches(metadata));

        for (entry, _meta) in filtered_iter {
            if let Some(ctx) = entry.context.as_ref() {
                let name_matches = ctx
                    .get("sensor")
                    .and_then(|v| v.as_str())
                    .is_some_and(|name| name == sensor_name);
                if name_matches {
                    if let Some(value) = ctx.get("value").and_then(|v| v.as_f64()) {
                        readings.push((entry.timestamp, value as f32));
                    }
                }
            }
        }

        readings.reverse();
        readings
    }
}

pub fn seed_sensor_history_from_log<S, E>(logger: &mut RingBufferLogger<S, E>)
where
    S: Storage<Error = E>,
    E: Debug,
{
    use crate::ui_types::SensorType;

    for (sensor_name, sensor_type) in [
        ("ph", SensorType::Ph),
        ("ec", SensorType::Conductivity),
        ("orp", SensorType::Orp),
        ("temperature", SensorType::Temperature),
    ] {
        let readings = logger.get_sensor_readings(sensor_name);
        let pairs: Vec<(i64, f32)> = readings
            .iter()
            .map(|(dt, val)| (dt.timestamp(), *val))
            .collect();
        crate::ui_backend::state::push_sensor_readings_bulk(sensor_type, &pairs);
    }
}

impl<S, E> Logger for RingBufferLogger<S, E>
where
    S: Storage<Error = E>,
    E: Debug,
{
    type Error = crate::storage::ring_buffer::RingBufferError<LogEntry, E>;

    fn log(
        &mut self,
        entry: LogEntry,
        level: LogLevel,
        category: LogCategory,
        error_type: Option<ErrorType>,
    ) -> Result<(), Self::Error> {
        if !self.should_log(level) {
            return Ok(());
        }

        let metadata = LogMetadata {
            published: false,
            log_level: level,
            category,
            error_type,
            reserved: [0; 8],
        };

        self.ring_buffer.write_record(&entry, metadata)
    }
}

#[derive(Debug, Clone)]
pub struct LogFilter {
    pub level: Option<LogLevel>,
    pub category: Option<LogCategory>,
    pub error_type: Option<ErrorType>,
    pub published_status: Option<bool>,
}

impl LogFilter {
    pub fn new() -> Self {
        Self {
            level: None,
            category: None,
            error_type: None,
            published_status: None,
        }
    }

    pub fn level(mut self, level: LogLevel) -> Self {
        self.level = Some(level);
        self
    }

    pub fn category(mut self, category: LogCategory) -> Self {
        self.category = Some(category);
        self
    }

    pub fn error_type(mut self, error_type: ErrorType) -> Self {
        self.error_type = Some(error_type);
        self
    }

    pub fn published(mut self, published: bool) -> Self {
        self.published_status = Some(published);
        self
    }

    pub fn matches(&self, metadata: &LogMetadata) -> bool {
        if let Some(level) = self.level {
            if metadata.log_level != level {
                return false;
            }
        }

        if let Some(category) = self.category {
            if metadata.category != category {
                return false;
            }
        }

        if let Some(error_type) = self.error_type {
            if metadata.error_type != Some(error_type) {
                return false;
            }
        }

        if let Some(published) = self.published_status {
            if metadata.published != published {
                return false;
            }
        }

        true
    }
}

impl Default for LogFilter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LogQuery<S, E> {
    ring_buffer: LogRingBuffer<S, E>,
}

impl<S, E> LogQuery<S, E>
where
    S: Storage<Error = E>,
    E: Debug,
{
    pub fn new(ring_buffer: LogRingBuffer<S, E>) -> Self {
        Self { ring_buffer }
    }

    pub fn get_logs_with_filter(&mut self, filter: LogFilter) -> Vec<(LogEntry, LogMetadata)> {
        let mut matching_logs = Vec::new();

        let filtered_iter = self
            .ring_buffer
            .iter_filtered(|metadata| filter.matches(metadata));

        for result in filtered_iter {
            matching_logs.push(result);
        }

        matching_logs
    }

    pub fn get_unpublished_logs(&mut self) -> Vec<(LogEntry, LogMetadata)> {
        self.get_logs_with_filter(LogFilter::new().published(false))
    }

    pub fn get_error_logs(&mut self) -> Vec<(LogEntry, LogMetadata)> {
        self.get_logs_with_filter(LogFilter::new().level(LogLevel::Error))
    }

    pub fn get_logs_by_category(&mut self, category: LogCategory) -> Vec<(LogEntry, LogMetadata)> {
        self.get_logs_with_filter(LogFilter::new().category(category))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ring_buffer::MockFlashStorage;
    use chrono::DateTime;

    fn test_timestamp() -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp_millis(0).unwrap()
    }

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry::new("Test message".to_string(), test_timestamp());
        assert_eq!(entry.message, "Test message");
        assert!(entry.context.is_none());
    }

    #[test]
    fn test_log_entry_with_context() {
        let mut context = serde_json::Map::new();
        context.insert(
            "sensor_id".to_string(),
            serde_json::Value::String("ph_sensor_1".to_string()),
        );
        context.insert(
            "voltage".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(3.14).unwrap()),
        );

        let entry = LogEntry::with_context(
            "Sensor reading".to_string(),
            test_timestamp(),
            context.clone(),
        );
        assert_eq!(entry.message, "Sensor reading");
        assert_eq!(entry.context, Some(context));
    }

    #[test]
    fn test_log_entry_add_context() {
        let mut entry = LogEntry::new("Test message".to_string(), test_timestamp());
        entry.add_context_value("key1".to_string(), "value1");
        entry.add_context_value("key2".to_string(), 42);

        let context = entry.context.unwrap();
        assert_eq!(
            context.get("key1"),
            Some(&serde_json::Value::String("value1".to_string()))
        );
        assert_eq!(
            context.get("key2"),
            Some(&serde_json::Value::Number(serde_json::Number::from(42)))
        );
    }

    #[test]
    fn test_timestamped_value_implementation() {
        let entry = LogEntry::new("Test".to_string(), test_timestamp());
        let timestamp = entry.get_written_timestamp();
        assert_eq!(timestamp, entry.timestamp);
    }

    #[test]
    fn test_ring_buffer_logger_creation() {
        let storage = MockFlashStorage::new(0, 4096, None);
        let ring_buffer = LogRingBuffer::new(0, 4096, storage).expect("test log addresses must be page-aligned");
        RingBufferLogger::new(ring_buffer);
    }

    #[test]
    fn test_logger_info() {
        let storage = MockFlashStorage::new(0, 4096, None);
        let ring_buffer = LogRingBuffer::new(0, 4096, storage).expect("test log addresses must be page-aligned");
        let mut logger = RingBufferLogger::new(ring_buffer);

        let result = logger.info(
            "Info message".to_string(),
            test_timestamp(),
            LogCategory::System,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_logger_warn() {
        let storage = MockFlashStorage::new(0, 4096, None);
        let ring_buffer = LogRingBuffer::new(0, 4096, storage).expect("test log addresses must be page-aligned");
        let mut logger = RingBufferLogger::new(ring_buffer);

        let result = logger.warn(
            "Warning message".to_string(),
            test_timestamp(),
            LogCategory::Sensor,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_logger_error() {
        let storage = MockFlashStorage::new(0, 4096, None);
        let ring_buffer = LogRingBuffer::new(0, 4096, storage).expect("test log addresses must be page-aligned");
        let mut logger = RingBufferLogger::new(ring_buffer);

        let result = logger.error(
            "Error message".to_string(),
            test_timestamp(),
            LogCategory::Hardware,
            ErrorType::Hardware,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_logger_debug() {
        let storage = MockFlashStorage::new(0, 4096, None);
        let ring_buffer = LogRingBuffer::new(0, 4096, storage).expect("test log addresses must be page-aligned");
        let mut logger = RingBufferLogger::new(ring_buffer);

        let result = logger.debug(
            "Debug message".to_string(),
            test_timestamp(),
            LogCategory::Dosing,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_logger_with_context() {
        let storage = MockFlashStorage::new(0, 4096, None);
        let ring_buffer = LogRingBuffer::new(0, 4096, storage).expect("test log addresses must be page-aligned");
        let mut logger = RingBufferLogger::new(ring_buffer);

        let mut context = serde_json::Map::new();
        context.insert(
            "ph_level".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(7.2).unwrap()),
        );

        let result = logger.info_with_context(
            "pH reading completed".to_string(),
            test_timestamp(),
            context,
            LogCategory::Sensor,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_direct_log_method() {
        let storage = MockFlashStorage::new(0, 4096, None);
        let ring_buffer = LogRingBuffer::new(0, 4096, storage).expect("test log addresses must be page-aligned");
        let mut logger = RingBufferLogger::new(ring_buffer);

        let entry = LogEntry::new("Direct log test".to_string(), test_timestamp());
        let result = logger.log(entry, LogLevel::Info, LogCategory::System, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_logger_default_log_level() {
        let storage = MockFlashStorage::new(0, 4096, None);
        let ring_buffer = LogRingBuffer::new(0, 4096, storage).expect("test log addresses must be page-aligned");
        let logger = RingBufferLogger::new(ring_buffer);
        assert_eq!(logger.get_log_level(), LogLevel::Info);
    }

    #[test]
    fn test_logger_should_log() {
        let storage = MockFlashStorage::new(0, 4096, None);
        let ring_buffer = LogRingBuffer::new(0, 4096, storage).expect("test log addresses must be page-aligned");
        let logger = RingBufferLogger::new(ring_buffer);

        // Should log at or above the configured level (Info by default)
        assert!(logger.should_log(LogLevel::Error));
        assert!(logger.should_log(LogLevel::Warn));
        assert!(logger.should_log(LogLevel::Info));
        assert!(!logger.should_log(LogLevel::Debug)); // Below threshold
    }

    #[test]
    fn test_logger_with_log_level() {
        let storage = MockFlashStorage::new(0, 4096, None);
        let ring_buffer = LogRingBuffer::new(0, 4096, storage).expect("test log addresses must be page-aligned");
        let mut logger = RingBufferLogger::with_log_level(ring_buffer, LogLevel::Warn); // Only warn and above

        // Should log warn and error
        assert!(logger
            .warn("Warning".to_string(), test_timestamp(), LogCategory::System)
            .is_ok());
        assert!(logger
            .error(
                "Error".to_string(),
                test_timestamp(),
                LogCategory::System,
                ErrorType::Software
            )
            .is_ok());

        // Should not log info (below threshold)
        assert!(logger
            .info("Info".to_string(), test_timestamp(), LogCategory::System)
            .is_ok()); // Returns Ok but doesn't actually log
    }

    #[test]
    fn test_log_filter_creation() {
        let filter = LogFilter::new()
            .level(LogLevel::Error)
            .category(LogCategory::Sensor)
            .error_type(ErrorType::Hardware)
            .published(false);

        assert_eq!(filter.level, Some(LogLevel::Error));
        assert_eq!(filter.category, Some(LogCategory::Sensor));
        assert_eq!(filter.error_type, Some(ErrorType::Hardware));
        assert_eq!(filter.published_status, Some(false));
    }

    #[test]
    fn test_log_filter_matches() {
        let filter = LogFilter::new()
            .level(LogLevel::Error)
            .category(LogCategory::Sensor);

        let matching_metadata = LogMetadata {
            published: false,
            log_level: LogLevel::Error,
            category: LogCategory::Sensor,
            error_type: Some(ErrorType::Hardware),
            reserved: [0; 8],
        };

        let non_matching_metadata = LogMetadata {
            published: false,
            log_level: LogLevel::Info, // Different level
            category: LogCategory::Sensor,
            error_type: Some(ErrorType::Hardware),
            reserved: [0; 8],
        };

        assert!(filter.matches(&matching_metadata));
        assert!(!filter.matches(&non_matching_metadata));
    }

    #[test]
    fn test_log_query_creation() {
        let storage = MockFlashStorage::new(0, 4096, None);
        let ring_buffer = LogRingBuffer::new(0, 4096, storage).expect("test log addresses must be page-aligned");
        let _query = LogQuery::new(ring_buffer);
        // Just testing creation
    }

    #[test]
    fn test_log_level_update() {
        let storage = MockFlashStorage::new(0, 4096, None);
        let ring_buffer = LogRingBuffer::new(0, 4096, storage).expect("test log addresses must be page-aligned");
        let mut logger = RingBufferLogger::new(ring_buffer);

        logger.set_log_level(LogLevel::Debug);
        assert_eq!(logger.get_log_level(), LogLevel::Debug);
    }

    #[test]
    fn test_error_propagates_to_runtime_state() {
        use crate::peripherals::SensorError;
        use crate::ui_types::SensorType;

        let _ = crate::state::init_clock(|| 1_000_000u64);

        let error = LoggableError::Sensor(SensorError::HardwareReadFailure(SensorType::Ph));
        flash_log_error(&error);

        let request = LOG_CHANNEL.try_receive().expect("LOG_CHANNEL should have a message");

        let storage = MockFlashStorage::new(0, 4096 * 4, None);
        let ring_buffer = LogRingBuffer::new(0, 4096 * 4, storage)
            .expect("test log addresses must be page-aligned");
        let mut logger = RingBufferLogger::new(ring_buffer);

        process_log_request(request, &mut logger);

        assert!(crate::ui_backend::state::take_errors_dirty());
        let errors = crate::ui_backend::state::get_recent_errors();
        assert!(!errors.is_empty(), "Expected at least one error in runtime state");
        let last = errors.last().unwrap();
        assert_eq!(last.category, LogCategory::Sensor);
        assert!(
            last.message.contains("pH"),
            "Expected error message to mention pH, got: {}",
            last.message
        );
        assert_eq!(last.timestamp_secs, 1);
    }

    #[test]
    fn test_sensor_reading_query() {
        let storage = MockFlashStorage::new(0, 4096 * 4, None);
        let ring_buffer = LogRingBuffer::new(0, 4096 * 4, storage).expect("test log addresses must be page-aligned");
        let mut logger = RingBufferLogger::new(ring_buffer);

        let ts1 = DateTime::<Utc>::from_timestamp_millis(1000).unwrap();
        let ts2 = DateTime::<Utc>::from_timestamp_millis(2000).unwrap();
        let ts3 = DateTime::<Utc>::from_timestamp_millis(3000).unwrap();

        let mut ph_ctx1 = serde_json::Map::new();
        ph_ctx1.insert("sensor".into(), serde_json::Value::String("ph".into()));
        ph_ctx1.insert("value".into(), serde_json::Value::Number(serde_json::Number::from_f64(7.2).unwrap()));
        logger.info_with_context(String::new(), ts1, ph_ctx1, LogCategory::Sensor).unwrap();

        let mut ec_ctx = serde_json::Map::new();
        ec_ctx.insert("sensor".into(), serde_json::Value::String("ec".into()));
        ec_ctx.insert("value".into(), serde_json::Value::Number(serde_json::Number::from_f64(1400.0).unwrap()));
        logger.info_with_context(String::new(), ts2, ec_ctx, LogCategory::Sensor).unwrap();

        let mut ph_ctx2 = serde_json::Map::new();
        ph_ctx2.insert("sensor".into(), serde_json::Value::String("ph".into()));
        ph_ctx2.insert("value".into(), serde_json::Value::Number(serde_json::Number::from_f64(6.8).unwrap()));
        logger.info_with_context(String::new(), ts3, ph_ctx2, LogCategory::Sensor).unwrap();

        let ph_readings = logger.get_sensor_readings("ph");
        assert_eq!(ph_readings.len(), 2);
        assert_eq!(ph_readings[0].0, ts1);
        assert!((ph_readings[0].1 - 7.2).abs() < 0.001);
        assert_eq!(ph_readings[1].0, ts3);
        assert!((ph_readings[1].1 - 6.8).abs() < 0.001);

        let ec_readings = logger.get_sensor_readings("ec");
        assert_eq!(ec_readings.len(), 1);
        assert!((ec_readings[0].1 - 1400.0).abs() < 0.1);

        let orp_readings = logger.get_sensor_readings("orp");
        assert_eq!(orp_readings.len(), 0);
    }
}
