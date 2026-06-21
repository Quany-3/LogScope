pub const MODULE_NAME: &str = "analyzer";

use crate::model::{ErrorPattern, LogEntry, LogLevel};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

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

    pub fn top_sources(&self, entries: &[LogEntry], limit: usize) -> Vec<SourceRanking> {
        let mut counts = HashMap::<String, usize>::new();
        for entry in entries {
            *counts.entry(entry.source.name.clone()).or_insert(0) += 1;
        }

        let mut rankings = counts
            .into_iter()
            .map(|(source, count)| SourceRanking { source, count })
            .collect::<Vec<_>>();
        rankings.sort_by(|left, right| {
            right
                .count
                .cmp(&left.count)
                .then_with(|| left.source.cmp(&right.source))
        });
        rankings.truncate(limit);
        rankings
    }

    pub fn top_error_patterns(&self, entries: &[LogEntry], limit: usize) -> Vec<ErrorPattern> {
        let mut grouped = BTreeMap::<String, (usize, String)>::new();
        for entry in entries.iter().filter(|entry| entry.level.is_error()) {
            let signature = error_signature(&entry.message);
            let group = grouped
                .entry(signature)
                .or_insert_with(|| (0, entry.message.clone()));
            group.0 += 1;
        }

        let mut patterns = grouped
            .into_iter()
            .map(|(signature, (occurrences, sample_message))| ErrorPattern {
                signature,
                occurrences,
                sample_message,
            })
            .collect::<Vec<_>>();
        patterns.sort_by(|left, right| {
            right
                .occurrences
                .cmp(&left.occurrences)
                .then_with(|| left.signature.cmp(&right.signature))
        });
        patterns.truncate(limit);
        patterns
    }

    pub fn detect_slow_requests<'a>(
        &self,
        entries: &'a [LogEntry],
        threshold_ms: u64,
    ) -> Vec<&'a LogEntry> {
        entries
            .iter()
            .filter(|entry| {
                entry
                    .fields
                    .get("duration_ms")
                    .and_then(|value| value.parse::<u64>().ok())
                    .is_some_and(|duration| duration >= threshold_ms)
            })
            .collect()
    }

    pub fn build_summary<'a>(
        &self,
        entries: &'a [LogEntry],
        ranking_limit: usize,
        slow_threshold_ms: u64,
    ) -> AnalysisSummary<'a> {
        AnalysisSummary {
            basic: self.analyze(entries),
            top_sources: self.top_sources(entries, ranking_limit),
            error_patterns: self.top_error_patterns(entries, ranking_limit),
            slow_requests: self.detect_slow_requests(entries, slow_threshold_ms),
        }
    }
}

fn error_signature(message: &str) -> String {
    let signature = message
        .split_whitespace()
        .filter(|token| !token.contains('='))
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ");

    if signature.is_empty() {
        message.trim().to_ascii_lowercase()
    } else {
        signature
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
    use crate::model::{ErrorPattern, LogEntry, LogLevel, LogSource, LogTimestamp};
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

    #[test]
    fn ranks_top_log_sources() {
        let rankings = BasicAnalyzer.top_sources(&advanced_mock_entries(), 2);

        assert_eq!(rankings.len(), 2);
        assert_eq!(rankings[0].source, "api");
        assert_eq!(rankings[0].count, 2);
        assert_eq!(rankings[1].source, "db");
    }

    #[test]
    fn groups_top_error_patterns() {
        let patterns = BasicAnalyzer.top_error_patterns(&advanced_mock_entries(), 3);

        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].signature, "database timeout");
        assert_eq!(patterns[0].occurrences, 2);
    }

    #[test]
    fn detects_slow_requests_from_structured_duration() {
        let entries = advanced_mock_entries();
        let slow = BasicAnalyzer.detect_slow_requests(&entries, 1_000);

        assert_eq!(slow.len(), 2);
        assert_eq!(slow[0].source.name, "worker");
        assert_eq!(slow[1].source.name, "api");
    }

    #[test]
    fn builds_advanced_analysis_summary() {
        let entries = advanced_mock_entries();
        let summary = BasicAnalyzer.build_summary(&entries, 2, 1_000);

        assert_eq!(summary.basic.total_count, 4);
        assert_eq!(summary.top_sources[0].source, "api");
        assert_eq!(
            summary.error_patterns[0],
            ErrorPattern {
                signature: "database timeout".to_string(),
                occurrences: 2,
                sample_message: "database timeout status=500 duration_ms=1500".to_string(),
            }
        );
        assert_eq!(summary.slow_requests.len(), 2);
    }

    /// Shared sample entries for analyzer unit tests.
    fn mock_log_entries() -> Vec<LogEntry> {
        vec![
            LogEntry {
                timestamp: LogTimestamp::new(Utc.with_ymd_and_hms(2026, 6, 12, 10, 0, 0).unwrap()),
                level: LogLevel::Info,
                source: LogSource::new("api"),
                message: "request completed".to_string(),
                fields: Default::default(),
                raw: "2026-06-12T10:00:00Z INFO api request completed".to_string(),
            },
            LogEntry {
                timestamp: LogTimestamp::new(Utc.with_ymd_and_hms(2026, 6, 12, 10, 1, 0).unwrap()),
                level: LogLevel::Warn,
                source: LogSource::new("worker"),
                message: "retrying failed job".to_string(),
                fields: Default::default(),
                raw: "2026-06-12T10:01:00Z WARN worker retrying failed job".to_string(),
            },
            LogEntry {
                timestamp: LogTimestamp::new(Utc.with_ymd_and_hms(2026, 6, 12, 10, 2, 0).unwrap()),
                level: LogLevel::Error,
                source: LogSource::new("api"),
                message: "database timeout".to_string(),
                fields: Default::default(),
                raw: "2026-06-12T10:02:00Z ERROR api database timeout".to_string(),
            },
        ]
    }

    fn advanced_mock_entries() -> Vec<LogEntry> {
        vec![
            advanced_entry(LogLevel::Info, "api", "request completed", 80),
            advanced_entry(LogLevel::Warn, "worker", "job delayed", 1_200),
            advanced_entry(LogLevel::Error, "api", "database timeout status=500", 1_500),
            advanced_entry(LogLevel::Error, "db", "database timeout status=503", 900),
        ]
    }

    fn advanced_entry(level: LogLevel, source: &str, message: &str, duration_ms: u64) -> LogEntry {
        let message = format!("{message} duration_ms={duration_ms}");
        LogEntry {
            timestamp: LogTimestamp::new(Utc.with_ymd_and_hms(2026, 6, 12, 12, 0, 0).unwrap()),
            level,
            source: LogSource::new(source),
            fields: [("duration_ms".to_string(), duration_ms.to_string())]
                .into_iter()
                .collect(),
            raw: message.clone(),
            message,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRanking {
    pub source: String,
    pub count: usize,
}

/// Combined basic and advanced metrics. Slow entries borrow the input collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisSummary<'a> {
    pub basic: AnalysisResult,
    pub top_sources: Vec<SourceRanking>,
    pub error_patterns: Vec<ErrorPattern>,
    pub slow_requests: Vec<&'a LogEntry>,
}
