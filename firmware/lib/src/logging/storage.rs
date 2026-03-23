cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::vec::Vec;
    } else {
        use alloc::vec::Vec;
    }
}

use crate::storage::ring_buffer::{MetadataSerialize, RingBuffer};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Info
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum LogCategory {
    Sensor = 0,
    Pump = 1,
    Dosing = 2,
    Network = 3,
    System = 4,
    Calibration = 5,
    Hardware = 6,
}

impl Default for LogCategory {
    fn default() -> Self {
        LogCategory::System
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum ErrorType {
    Hardware = 0,
    Configuration = 1,
    Network = 2,
    Software = 3,
}

impl Default for ErrorType {
    fn default() -> Self {
        ErrorType::Software
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct LogMetadata {
    pub published: bool,
    pub log_level: LogLevel,
    pub category: LogCategory,
    pub error_type: Option<ErrorType>,
    pub reserved: [u8; 8],
}

impl Default for LogMetadata {
    fn default() -> Self {
        Self {
            published: false,
            log_level: LogLevel::Info,
            category: LogCategory::System,
            error_type: None,
            reserved: [0; 8],
        }
    }
}

impl MetadataSerialize for LogMetadata {
    fn serialize_to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(12);
        bytes.push(self.published as u8);
        bytes.push(self.log_level as u8);
        bytes.push(self.category as u8);
        bytes.push(match self.error_type {
            Some(error_type) => error_type as u8,
            None => 255,
        });
        bytes.extend_from_slice(&self.reserved);
        bytes
    }

    fn deserialize_from_bytes(bytes: &[u8]) -> Self {
        assert_eq!(bytes.len(), 12, "LogMetadata must be exactly 12 bytes");

        let published = bytes[0] != 0;
        let log_level = match bytes[1] {
            0 => LogLevel::Error,
            1 => LogLevel::Warn,
            2 => LogLevel::Info,
            3 => LogLevel::Debug,
            _ => LogLevel::Info,
        };
        let category = match bytes[2] {
            0 => LogCategory::Sensor,
            1 => LogCategory::Pump,
            2 => LogCategory::Dosing,
            3 => LogCategory::Network,
            4 => LogCategory::System,
            5 => LogCategory::Calibration,
            6 => LogCategory::Hardware,
            _ => LogCategory::System,
        };
        let error_type = match bytes[3] {
            0 => Some(ErrorType::Hardware),
            1 => Some(ErrorType::Configuration),
            2 => Some(ErrorType::Network),
            3 => Some(ErrorType::Software),
            255 => None,
            _ => None,
        };

        let mut reserved = [0u8; 8];
        reserved.copy_from_slice(&bytes[4..12]);

        Self {
            published,
            log_level,
            category,
            error_type,
            reserved,
        }
    }

    fn serialized_size() -> usize {
        12
    }
}

pub type LogRingBuffer<S, E> = RingBuffer<super::LogEntry, LogMetadata, S, E>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ring_buffer::{
        MockFlashStorage,
        MockFlashStorageError,
    };
    use crate::logging::LogEntry;
    use chrono::{DateTime, Utc};
    use proptest::prelude::*;
    use std::vec;

    fn test_timestamp(millis: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp_millis(millis).unwrap()
    }

    fn create_log_entry(message: &str, millis: i64) -> LogEntry {
        LogEntry::new(message.to_string(), test_timestamp(millis))
    }

    fn create_metadata(level: LogLevel, category: LogCategory, published: bool) -> LogMetadata {
        LogMetadata {
            published,
            log_level: level,
            category,
            error_type: None,
            reserved: [0; 8],
        }
    }

    fn get_log_ring_buffer(
        storage: Option<MockFlashStorage>,
    ) -> LogRingBuffer<MockFlashStorage, MockFlashStorageError> {
        let start_address = 0x0000;
        let end_address = 0x4000;
        let flash = match storage {
            None => MockFlashStorage::new(start_address, end_address, None),
            Some(existing) => existing,
        };
        LogRingBuffer::new(start_address, end_address, flash).expect("test log addresses must be page-aligned")
    }

    #[test]
    fn test_log_level_serialization_roundtrip() {
        let levels = vec![
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Info,
            LogLevel::Debug,
        ];

        for level in levels {
            let metadata = LogMetadata {
                log_level: level,
                ..Default::default()
            };
            let bytes = metadata.serialize_to_bytes();
            let deserialized = LogMetadata::deserialize_from_bytes(&bytes);
            assert_eq!(deserialized.log_level, level);
        }
    }

    #[test]
    fn test_log_category_serialization_roundtrip() {
        let categories = vec![
            LogCategory::Sensor,
            LogCategory::Pump,
            LogCategory::Dosing,
            LogCategory::Network,
            LogCategory::System,
            LogCategory::Calibration,
            LogCategory::Hardware,
        ];

        for category in categories {
            let metadata = LogMetadata {
                category,
                ..Default::default()
            };
            let bytes = metadata.serialize_to_bytes();
            let deserialized = LogMetadata::deserialize_from_bytes(&bytes);
            assert_eq!(deserialized.category, category);
        }
    }

    #[test]
    fn test_error_type_serialization_roundtrip() {
        let error_types = vec![
            Some(ErrorType::Hardware),
            Some(ErrorType::Configuration),
            Some(ErrorType::Network),
            Some(ErrorType::Software),
            None,
        ];

        for error_type in error_types {
            let metadata = LogMetadata {
                error_type,
                ..Default::default()
            };
            let bytes = metadata.serialize_to_bytes();
            let deserialized = LogMetadata::deserialize_from_bytes(&bytes);
            assert_eq!(deserialized.error_type, error_type);
        }
    }

    #[test]
    fn test_published_flag_serialization() {
        for published in [true, false] {
            let metadata = LogMetadata {
                published,
                ..Default::default()
            };
            let bytes = metadata.serialize_to_bytes();
            let deserialized = LogMetadata::deserialize_from_bytes(&bytes);
            assert_eq!(deserialized.published, published);
        }
    }

    #[test]
    fn test_reserved_bytes_preserved() {
        let reserved = [1, 2, 3, 4, 5, 6, 7, 8];
        let metadata = LogMetadata {
            reserved,
            ..Default::default()
        };
        let bytes = metadata.serialize_to_bytes();
        let deserialized = LogMetadata::deserialize_from_bytes(&bytes);
        assert_eq!(deserialized.reserved, reserved);
    }

    #[test]
    fn test_full_metadata_serialization() {
        let metadata = LogMetadata {
            published: true,
            log_level: LogLevel::Error,
            category: LogCategory::Hardware,
            error_type: Some(ErrorType::Hardware),
            reserved: [255; 8],
        };
        let bytes = metadata.serialize_to_bytes();
        let deserialized = LogMetadata::deserialize_from_bytes(&bytes);

        assert_eq!(deserialized.published, true);
        assert_eq!(deserialized.log_level, LogLevel::Error);
        assert_eq!(deserialized.category, LogCategory::Hardware);
        assert_eq!(deserialized.error_type, Some(ErrorType::Hardware));
        assert_eq!(deserialized.reserved, [255; 8]);
    }

    #[test]
    fn test_metadata_size() {
        assert_eq!(LogMetadata::serialized_size(), 12);
    }

    #[test]
    fn test_log_ring_buffer_write_and_read() {
        let mut buffer = get_log_ring_buffer(None);
        let entry = create_log_entry("Test message", 1722581155825);
        let metadata = create_metadata(LogLevel::Info, LogCategory::System, false);

        buffer.write_record(&entry, metadata).unwrap();
        let result = buffer.read_latest_record().unwrap().unwrap();

        assert_eq!(result.0.message, "Test message");
        assert_eq!(result.1.log_level, LogLevel::Info);
        assert_eq!(result.1.category, LogCategory::System);
    }

    #[test]
    fn test_filter_by_log_level() {
        let mut buffer = get_log_ring_buffer(None);

        let entries = vec![
            (create_log_entry("info1", 1722581155825), create_metadata(LogLevel::Info, LogCategory::System, false)),
            (create_log_entry("error1", 1722581155826), create_metadata(LogLevel::Error, LogCategory::Sensor, false)),
            (create_log_entry("info2", 1722581155827), create_metadata(LogLevel::Info, LogCategory::Network, false)),
            (create_log_entry("error2", 1722581155828), create_metadata(LogLevel::Error, LogCategory::Pump, false)),
            (create_log_entry("warn1", 1722581155829), create_metadata(LogLevel::Warn, LogCategory::Dosing, false)),
        ];

        for (entry, metadata) in &entries {
            buffer.write_record(entry, *metadata).unwrap();
        }

        let error_logs: Vec<_> = buffer
            .iter_filtered(|meta| meta.log_level == LogLevel::Error)
            .collect();

        assert_eq!(error_logs.len(), 2);
        assert_eq!(error_logs[0].0.message, "error2");
        assert_eq!(error_logs[1].0.message, "error1");
    }

    #[test]
    fn test_filter_by_category() {
        let mut buffer = get_log_ring_buffer(None);

        let entries = vec![
            (create_log_entry("sys1", 1722581155825), create_metadata(LogLevel::Info, LogCategory::System, false)),
            (create_log_entry("sensor1", 1722581155826), create_metadata(LogLevel::Error, LogCategory::Sensor, false)),
            (create_log_entry("sys2", 1722581155827), create_metadata(LogLevel::Warn, LogCategory::System, false)),
            (create_log_entry("pump1", 1722581155828), create_metadata(LogLevel::Error, LogCategory::Pump, false)),
            (create_log_entry("sensor2", 1722581155829), create_metadata(LogLevel::Info, LogCategory::Sensor, false)),
        ];

        for (entry, metadata) in &entries {
            buffer.write_record(entry, *metadata).unwrap();
        }

        let sensor_logs: Vec<_> = buffer
            .iter_filtered(|meta| meta.category == LogCategory::Sensor)
            .collect();

        assert_eq!(sensor_logs.len(), 2);
        assert_eq!(sensor_logs[0].0.message, "sensor2");
        assert_eq!(sensor_logs[1].0.message, "sensor1");
    }

    #[test]
    fn test_filter_by_published_status() {
        let mut buffer = get_log_ring_buffer(None);

        let entries = vec![
            (create_log_entry("unpub1", 1722581155825), create_metadata(LogLevel::Info, LogCategory::System, false)),
            (create_log_entry("pub1", 1722581155826), create_metadata(LogLevel::Error, LogCategory::Sensor, true)),
            (create_log_entry("unpub2", 1722581155827), create_metadata(LogLevel::Warn, LogCategory::System, false)),
            (create_log_entry("pub2", 1722581155828), create_metadata(LogLevel::Error, LogCategory::Pump, true)),
        ];

        for (entry, metadata) in &entries {
            buffer.write_record(entry, *metadata).unwrap();
        }

        let unpublished: Vec<_> = buffer.iter_filtered(|meta| !meta.published).collect();
        assert_eq!(unpublished.len(), 2);
        assert_eq!(unpublished[0].0.message, "unpub2");
        assert_eq!(unpublished[1].0.message, "unpub1");

        let published: Vec<_> = buffer.iter_filtered(|meta| meta.published).collect();
        assert_eq!(published.len(), 2);
        assert_eq!(published[0].0.message, "pub2");
        assert_eq!(published[1].0.message, "pub1");
    }

    #[test]
    fn test_filter_complex_predicate() {
        let mut buffer = get_log_ring_buffer(None);

        let entries = vec![
            (create_log_entry("match1", 1722581155825), create_metadata(LogLevel::Error, LogCategory::Sensor, false)),
            (create_log_entry("no_match1", 1722581155826), create_metadata(LogLevel::Error, LogCategory::System, false)),
            (create_log_entry("no_match2", 1722581155827), create_metadata(LogLevel::Info, LogCategory::Sensor, false)),
            (create_log_entry("match2", 1722581155828), create_metadata(LogLevel::Error, LogCategory::Sensor, true)),
            (create_log_entry("no_match3", 1722581155829), create_metadata(LogLevel::Warn, LogCategory::Pump, false)),
        ];

        for (entry, metadata) in &entries {
            buffer.write_record(entry, *metadata).unwrap();
        }

        let filtered: Vec<_> = buffer
            .iter_filtered(|meta| {
                meta.log_level == LogLevel::Error && meta.category == LogCategory::Sensor
            })
            .collect();

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].0.message, "match2");
        assert_eq!(filtered[1].0.message, "match1");
    }

    #[test]
    fn test_all_log_levels_in_buffer() {
        let mut buffer = get_log_ring_buffer(None);
        let levels = vec![
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Info,
            LogLevel::Debug,
        ];

        for (i, level) in levels.iter().enumerate() {
            let entry = create_log_entry(&format!("level_{}", i), 1722581155825 + i as i64);
            let metadata = create_metadata(*level, LogCategory::System, false);
            buffer.write_record(&entry, metadata).unwrap();

            let result = buffer.read_latest_record().unwrap().unwrap();
            assert_eq!(result.1.log_level, *level);
        }
    }

    #[test]
    fn test_all_categories_in_buffer() {
        let mut buffer = get_log_ring_buffer(None);
        let categories = vec![
            LogCategory::Sensor,
            LogCategory::Pump,
            LogCategory::Dosing,
            LogCategory::Network,
            LogCategory::System,
            LogCategory::Calibration,
            LogCategory::Hardware,
        ];

        for (i, category) in categories.iter().enumerate() {
            let entry = create_log_entry(&format!("cat_{}", i), 1722581155825 + i as i64);
            let metadata = create_metadata(LogLevel::Info, *category, false);
            buffer.write_record(&entry, metadata).unwrap();

            let result = buffer.read_latest_record().unwrap().unwrap();
            assert_eq!(result.1.category, *category);
        }
    }

    #[test]
    fn test_all_error_types_in_buffer() {
        let mut buffer = get_log_ring_buffer(None);
        let error_types = vec![
            Some(ErrorType::Hardware),
            Some(ErrorType::Configuration),
            Some(ErrorType::Network),
            Some(ErrorType::Software),
            None,
        ];

        for (i, error_type) in error_types.iter().enumerate() {
            let entry = create_log_entry(&format!("err_{}", i), 1722581155825 + i as i64);
            let mut metadata = create_metadata(LogLevel::Error, LogCategory::System, false);
            metadata.error_type = *error_type;
            buffer.write_record(&entry, metadata).unwrap();

            let result = buffer.read_latest_record().unwrap().unwrap();
            assert_eq!(result.1.error_type, *error_type);
        }
    }

    #[test]
    fn test_metadata_with_max_values() {
        let mut buffer = get_log_ring_buffer(None);
        let entry = create_log_entry("max_values", 1722581155825);
        let metadata = LogMetadata {
            published: true,
            log_level: LogLevel::Error,
            category: LogCategory::Calibration,
            error_type: Some(ErrorType::Hardware),
            reserved: [255; 8],
        };

        buffer.write_record(&entry, metadata).unwrap();
        let result = buffer.read_latest_record().unwrap().unwrap();

        assert_eq!(result.1.reserved, [255; 8]);
        assert_eq!(result.1.log_level, LogLevel::Error);
        assert_eq!(result.1.category, LogCategory::Calibration);
        assert_eq!(result.1.error_type, Some(ErrorType::Hardware));
    }

    fn arb_log_entry() -> impl Strategy<Value = LogEntry> {
        (".{0,200}", 0i64..=4_102_444_800_000i64).prop_map(|(message, millis)| {
            let timestamp = DateTime::<Utc>::from_timestamp_millis(millis).unwrap();
            LogEntry::new(message, timestamp)
        })
    }

    proptest! {
        #[test]
        fn test_single_log_write_read_roundtrip(
            entry in arb_log_entry(),
            metadata: LogMetadata
        ) {
            let mut buffer = get_log_ring_buffer(None);

            buffer.write_record(&entry, metadata).unwrap();
            let (read_entry, read_metadata) = buffer.read_latest_record().unwrap().unwrap();

            prop_assert_eq!(read_entry, entry);
            prop_assert_eq!(read_metadata, metadata);
        }
    }
}
