pub const MODULE_NAME: &str = "model";

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
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

/// Optional constraints that can be combined by search and filter workflows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterCondition {
    pub keyword: Option<String>,
    pub level: Option<LogLevel>,
    pub source: Option<String>,
    pub start_time: Option<LogTimestamp>,
    pub end_time: Option<LogTimestamp>,
}

impl FilterCondition {
    pub fn is_empty(&self) -> bool {
        self.keyword.is_none()
            && self.level.is_none()
            && self.source.is_none()
            && self.start_time.is_none()
            && self.end_time.is_none()
    }
}

/// Search matches borrowing their original parsed log entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult<'a> {
    pub entries: Vec<&'a LogEntry>,
    pub total_matches: usize,
}

impl<'a> SearchResult<'a> {
    pub fn new(entries: Vec<&'a LogEntry>) -> Self {
        let total_matches = entries.len();
        Self {
            entries,
            total_matches,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ErrorPattern, FilterCondition, LogEntry, LogLevel, LogSource, LogTimestamp, SearchResult,
    };
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
            fields: [("status".to_string(), "500".to_string())]
                .into_iter()
                .collect(),
            raw: "2026-06-12T10:30:00Z ERROR api request failed".to_string(),
        };

        assert_eq!(entry.timestamp.value, timestamp);
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.source.name, "api");
        assert_eq!(entry.message, "request failed");
        assert_eq!(entry.fields["status"], "500");
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

    #[test]
    fn defines_filter_condition_model() {
        let start = LogTimestamp::new(Utc.with_ymd_and_hms(2026, 6, 12, 10, 0, 0).unwrap());
        let end = LogTimestamp::new(Utc.with_ymd_and_hms(2026, 6, 12, 11, 0, 0).unwrap());
        let condition = FilterCondition {
            keyword: Some("timeout".to_string()),
            level: Some(LogLevel::Error),
            source: Some("api".to_string()),
            start_time: Some(start),
            end_time: Some(end),
        };

        assert_eq!(condition.keyword.as_deref(), Some("timeout"));
        assert_eq!(condition.level, Some(LogLevel::Error));
        assert!(!condition.is_empty());
        assert!(FilterCondition::default().is_empty());
    }

    #[test]
    fn defines_borrowed_search_result_model() {
        let entry = LogEntry {
            timestamp: LogTimestamp::new(Utc.with_ymd_and_hms(2026, 6, 12, 10, 30, 0).unwrap()),
            level: LogLevel::Error,
            source: LogSource::new("api"),
            message: "database timeout".to_string(),
            fields: Default::default(),
            raw: "2026-06-12T10:30:00Z ERROR api database timeout".to_string(),
        };
        let result = SearchResult::new(vec![&entry]);

        assert_eq!(result.total_matches, 1);
        assert_eq!(result.entries[0].message, "database timeout");
        assert!(!result.is_empty());
    }

    #[test]
    fn defines_error_pattern_model() {
        let pattern = ErrorPattern::new("database timeout", "database timeout status=500");

        assert_eq!(pattern.signature, "database timeout");
        assert_eq!(pattern.occurrences, 1);
        assert_eq!(pattern.sample_message, "database timeout status=500");
    }
}

/// Repeated error signature produced by advanced analysis.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ErrorPattern {
    pub signature: String,
    pub occurrences: usize,
    pub sample_message: String,
}

impl ErrorPattern {
    pub fn new(signature: impl Into<String>, sample_message: impl Into<String>) -> Self {
        Self {
            signature: signature.into(),
            occurrences: 1,
            sample_message: sample_message.into(),
        }
    }
}
