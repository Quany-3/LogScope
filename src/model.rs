pub const MODULE_NAME: &str = "model";

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Severity used for grouping, filtering, and ranking log records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    /// Convert common log labels into the canonical enum representation.
    pub fn from_label(label: &str) -> Option<Self> {
        match label.trim().to_ascii_uppercase().as_str() {
            "TRACE" => Some(Self::Trace),
            "DEBUG" => Some(Self::Debug),
            "INFO" => Some(Self::Info),
            "WARN" | "WARNING" => Some(Self::Warn),
            "ERROR" => Some(Self::Error),
            "FATAL" => Some(Self::Fatal),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        }
    }

    pub const fn is_error(self) -> bool {
        matches!(self, Self::Error | Self::Fatal)
    }
}

/// One normalized log record produced by parsers and consumed by analyzers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: LogTimestamp,
    pub level: LogLevel,
    pub source: LogSource,
    pub message: String,
    pub raw: String,
}

/// Logical origin of a log record, such as a service, file, or component.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LogSource {
    pub name: String,
}

impl LogSource {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// UTC timestamp wrapper used to keep parsed time values explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LogTimestamp {
    pub value: DateTime<Utc>,
}

impl LogTimestamp {
    pub fn new(value: DateTime<Utc>) -> Self {
        Self { value }
    }
}

#[cfg(test)]
mod tests {
    use super::{LogEntry, LogLevel, LogSource, LogTimestamp};
    use chrono::{TimeZone, Utc};

    #[test]
    fn defines_standard_log_levels() {
        let levels = [
            LogLevel::Trace,
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
            LogLevel::Fatal,
        ];

        assert_eq!(levels.len(), 6);
    }

    #[test]
    fn defines_log_entry_structure() {
        let timestamp = Utc.with_ymd_and_hms(2026, 6, 12, 10, 30, 0).unwrap();
        let entry = LogEntry {
            timestamp: LogTimestamp::new(timestamp),
            level: LogLevel::Error,
            source: LogSource::new("api"),
            message: "request failed".to_string(),
            raw: "2026-06-12T10:30:00Z ERROR api request failed".to_string(),
        };

        assert_eq!(entry.timestamp.value, timestamp);
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.source.name, "api");
        assert_eq!(entry.message, "request failed");
        assert!(entry.raw.contains("ERROR"));
    }

    #[test]
    fn defines_log_source_and_timestamp_models() {
        let source = LogSource::new("worker-1");
        let timestamp = Utc.with_ymd_and_hms(2026, 6, 12, 11, 0, 0).unwrap();
        let logged_at = LogTimestamp::new(timestamp);

        assert_eq!(source.name, "worker-1");
        assert_eq!(logged_at.value, timestamp);
    }

    #[test]
    fn provides_log_level_helpers() {
        assert_eq!(LogLevel::from_label("warning"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_label("error"), Some(LogLevel::Error));
        assert_eq!(LogLevel::Error.as_str(), "ERROR");
        assert!(LogLevel::Error.is_error());
        assert!(LogLevel::Info < LogLevel::Error);
    }
}
