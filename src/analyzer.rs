pub const MODULE_NAME: &str = "analyzer";

use crate::model::{LogEntry, LogLevel};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Aggregated metrics shared by CLI output, reports, and the TUI summary panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub total_count: usize,
    pub level_counts: HashMap<LogLevel, usize>,
    pub source_counts: HashMap<String, usize>,
}

impl AnalysisResult {
    pub fn new(total_count: usize) -> Self {
        Self {
            total_count,
            level_counts: HashMap::new(),
            source_counts: HashMap::new(),
        }
    }
}

/// Interface implemented by services that aggregate parsed log entries.
pub trait AnalysisService {
    fn analyze(&self, entries: &[LogEntry]) -> AnalysisResult;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct BasicAnalyzer;

impl BasicAnalyzer {
    /// Search the original log line without cloning matching entries.
    pub fn search_keyword<'a>(&self, entries: &'a [LogEntry], keyword: &str) -> Vec<&'a LogEntry> {
        let keyword = keyword.to_lowercase();
        entries
            .iter()
            .filter(|entry| entry.raw.to_lowercase().contains(&keyword))
            .collect()
    }

    pub fn filter_by_level<'a>(
        &self,
        entries: &'a [LogEntry],
        level: LogLevel,
    ) -> Vec<&'a LogEntry> {
        entries
            .iter()
            .filter(|entry| entry.level == level)
            .collect()
    }

    pub fn filter_by_source<'a>(&self, entries: &'a [LogEntry], source: &str) -> Vec<&'a LogEntry> {
        entries
            .iter()
            .filter(|entry| entry.source.name == source)
            .collect()
    }

    pub fn filter_by_time_range<'a>(
        &self,
        entries: &'a [LogEntry],
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<&'a LogEntry> {
        entries
            .iter()
            .filter(|entry| entry.timestamp.value >= start && entry.timestamp.value <= end)
            .collect()
    }
}

impl AnalysisService for BasicAnalyzer {
    fn analyze(&self, entries: &[LogEntry]) -> AnalysisResult {
        let mut result = AnalysisResult::new(entries.len());

        for entry in entries {
            *result.level_counts.entry(entry.level).or_insert(0) += 1;
            *result
                .source_counts
                .entry(entry.source.name.clone())
                .or_insert(0) += 1;
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::{AnalysisResult, AnalysisService, BasicAnalyzer};
    use crate::model::{LogEntry, LogLevel, LogSource, LogTimestamp};
    use chrono::{TimeZone, Utc};

    #[test]
    fn defines_analysis_result_models() {
        let mut result = AnalysisResult::new(3);
        result.level_counts.insert(LogLevel::Info, 2);
        result.level_counts.insert(LogLevel::Error, 1);
        result.source_counts.insert("api".to_string(), 2);

        assert_eq!(result.total_count, 3);
        assert_eq!(result.level_counts[&LogLevel::Info], 2);
        assert_eq!(result.source_counts["api"], 2);
    }

    #[test]
    fn provides_mock_log_entries_for_analyzer_tests() {
        let entries = mock_log_entries();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].level, LogLevel::Info);
        assert_eq!(entries[1].level, LogLevel::Warn);
        assert_eq!(entries[2].level, LogLevel::Error);
        assert_eq!(entries[2].source.name, "api");
    }

    #[test]
    fn analyzes_total_level_and_source_statistics() {
        let result = BasicAnalyzer.analyze(&mock_log_entries());

        assert_eq!(result.total_count, 3);
        assert_eq!(result.level_counts[&LogLevel::Info], 1);
        assert_eq!(result.level_counts[&LogLevel::Warn], 1);
        assert_eq!(result.level_counts[&LogLevel::Error], 1);
        assert_eq!(result.source_counts["api"], 2);
        assert_eq!(result.source_counts["worker"], 1);
    }

    #[test]
    fn analyzes_empty_log_collection() {
        let result = BasicAnalyzer.analyze(&[]);

        assert_eq!(result, AnalysisResult::new(0));
    }

    #[test]
    fn searches_entries_by_keyword_case_insensitively() {
        let entries = mock_log_entries();
        let matches = BasicAnalyzer.search_keyword(&entries, "FAILED");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].source.name, "worker");
    }

    #[test]
    fn filters_entries_by_level() {
        let entries = mock_log_entries();
        let matches = BasicAnalyzer.filter_by_level(&entries, LogLevel::Error);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message, "database timeout");
    }

    #[test]
    fn filters_entries_by_source() {
        let entries = mock_log_entries();
        let matches = BasicAnalyzer.filter_by_source(&entries, "api");

        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|entry| entry.source.name == "api"));
    }

    #[test]
    fn filters_entries_by_inclusive_time_range() {
        let entries = mock_log_entries();
        let start = Utc.with_ymd_and_hms(2026, 6, 12, 10, 1, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 6, 12, 10, 2, 0).unwrap();
        let matches = BasicAnalyzer.filter_by_time_range(&entries, start, end);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].level, LogLevel::Warn);
        assert_eq!(matches[1].level, LogLevel::Error);
    }

    /// Shared sample entries for analyzer unit tests.
    fn mock_log_entries() -> Vec<LogEntry> {
        vec![
            LogEntry {
                timestamp: LogTimestamp::new(Utc.with_ymd_and_hms(2026, 6, 12, 10, 0, 0).unwrap()),
                level: LogLevel::Info,
                source: LogSource::new("api"),
                message: "request completed".to_string(),
                raw: "2026-06-12T10:00:00Z INFO api request completed".to_string(),
            },
            LogEntry {
                timestamp: LogTimestamp::new(Utc.with_ymd_and_hms(2026, 6, 12, 10, 1, 0).unwrap()),
                level: LogLevel::Warn,
                source: LogSource::new("worker"),
                message: "retrying failed job".to_string(),
                raw: "2026-06-12T10:01:00Z WARN worker retrying failed job".to_string(),
            },
            LogEntry {
                timestamp: LogTimestamp::new(Utc.with_ymd_and_hms(2026, 6, 12, 10, 2, 0).unwrap()),
                level: LogLevel::Error,
                source: LogSource::new("api"),
                message: "database timeout".to_string(),
                raw: "2026-06-12T10:02:00Z ERROR api database timeout".to_string(),
            },
        ]
    }
}
