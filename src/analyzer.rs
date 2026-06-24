pub const MODULE_NAME: &str = "analyzer";

use crate::model::{ErrorPattern, LogEntry, LogLevel};
use chrono::{DateTime, Duration, Utc};
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

    pub fn realtime_summary(&self, entries: &[LogEntry], recent_limit: usize) -> RealtimeSummary {
        RealtimeSummary {
            total_count: entries.len(),
            warning_count: count_level(entries, LogLevel::Warn),
            error_count: entries
                .iter()
                .filter(|entry| entry.level.is_error())
                .count(),
            top_sources: self.top_sources(entries, recent_limit),
            recent_lines: entries
                .iter()
                .rev()
                .take(recent_limit)
                .map(LogEntry::display_line)
                .collect(),
        }
    }

    pub fn build_insights(
        &self,
        entries: &[LogEntry],
        window_seconds: i64,
        slow_threshold_ms: u64,
        correlation_limit: usize,
    ) -> OperationalInsights {
        let total_count = entries.len();
        let error_count = entries
            .iter()
            .filter(|entry| entry.level.is_error())
            .count();
        let fatal_count = count_level(entries, LogLevel::Fatal);
        let slow_count = self.detect_slow_requests(entries, slow_threshold_ms).len();
        let error_rate_percent = percent(error_count, total_count);
        let slow_rate_percent = percent(slow_count, total_count);
        let peak_window = peak_window(entries, window_seconds.max(1));
        let correlations = correlated_activity(entries, correlation_limit);
        let severity_score = severity_score(
            error_rate_percent,
            slow_rate_percent,
            fatal_count,
            peak_window.as_ref(),
        );

        OperationalInsights {
            severity_score,
            error_rate_percent,
            slow_rate_percent,
            peak_window,
            correlations,
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

fn count_level(entries: &[LogEntry], level: LogLevel) -> usize {
    entries.iter().filter(|entry| entry.level == level).count()
}

fn percent(count: usize, total: usize) -> u8 {
    count
        .saturating_mul(100)
        .checked_div(total)
        .unwrap_or_default()
        .min(100) as u8
}

fn severity_score(
    error_rate_percent: u8,
    slow_rate_percent: u8,
    fatal_count: usize,
    peak_window: Option<&TimeWindowInsight>,
) -> u8 {
    let peak_pressure = peak_window
        .map(|window| {
            window.error_count.saturating_mul(10) + window.warning_count.saturating_mul(4)
        })
        .unwrap_or_default();
    (usize::from(error_rate_percent)
        + usize::from(slow_rate_percent / 2)
        + fatal_count * 30
        + peak_pressure)
        .min(100) as u8
}

fn peak_window(entries: &[LogEntry], window_seconds: i64) -> Option<TimeWindowInsight> {
    let mut ordered = entries.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|entry| entry.timestamp.value);
    let mut best: Option<TimeWindowInsight> = None;

    for (index, first) in ordered.iter().enumerate() {
        let end = first.timestamp.value + Duration::seconds(window_seconds);
        let window_entries = ordered[index..]
            .iter()
            .take_while(|entry| entry.timestamp.value <= end)
            .copied()
            .collect::<Vec<_>>();
        let entry_count = window_entries.len();
        let error_count = window_entries
            .iter()
            .filter(|entry| entry.level.is_error())
            .count();
        let warning_count = window_entries
            .iter()
            .filter(|entry| entry.level == LogLevel::Warn)
            .count();
        let candidate = TimeWindowInsight {
            start: first.timestamp.value,
            end,
            entry_count,
            error_count,
            warning_count,
        };

        let replace = best.as_ref().is_none_or(|current| {
            candidate
                .error_count
                .cmp(&current.error_count)
                .then_with(|| candidate.warning_count.cmp(&current.warning_count))
                .then_with(|| candidate.entry_count.cmp(&current.entry_count))
                .is_gt()
        });
        if replace {
            best = Some(candidate);
        }
    }

    best
}

fn correlated_activity(entries: &[LogEntry], limit: usize) -> Vec<CorrelationInsight> {
    let mut grouped = BTreeMap::<String, Vec<&LogEntry>>::new();
    for entry in entries {
        for key in ["request_id", "job_id"] {
            if let Some(value) = entry.fields.get(key) {
                grouped
                    .entry(format!("{key}={value}"))
                    .or_default()
                    .push(entry);
            }
        }
    }

    let mut groups = grouped
        .into_iter()
        .filter(|(_, entries)| entries.len() > 1)
        .map(|(key, entries)| CorrelationInsight {
            key,
            entry_count: entries.len(),
            error_count: entries
                .iter()
                .filter(|entry| entry.level.is_error())
                .count(),
            sources: unique_sources(&entries),
            sample_messages: entries
                .iter()
                .take(3)
                .map(|entry| entry.message.clone())
                .collect(),
        })
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        right
            .error_count
            .cmp(&left.error_count)
            .then_with(|| right.entry_count.cmp(&left.entry_count))
            .then_with(|| left.key.cmp(&right.key))
    });
    groups.truncate(limit);
    groups
}

fn unique_sources(entries: &[&LogEntry]) -> Vec<String> {
    let mut sources = entries
        .iter()
        .map(|entry| entry.source.name.clone())
        .collect::<Vec<_>>();
    sources.sort();
    sources.dedup();
    sources
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
    use chrono::{Duration, TimeZone, Utc};

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

    #[test]
    fn builds_realtime_summary_for_tui_panels() {
        let entries = advanced_mock_entries();
        let summary = BasicAnalyzer.realtime_summary(&entries, 2);

        assert_eq!(summary.total_count, 4);
        assert_eq!(summary.warning_count, 1);
        assert_eq!(summary.error_count, 2);
        assert_eq!(summary.top_sources[0].source, "api");
        assert_eq!(summary.top_sources[0].count, 2);
        assert_eq!(summary.recent_lines.len(), 2);
        assert_eq!(
            summary.recent_lines[0],
            "2026-06-12T12:00:00Z ERROR db database timeout status=503 duration_ms=900"
        );
    }

    #[test]
    fn builds_operational_insights_from_log_activity() {
        let entries = insight_mock_entries();
        let insights = BasicAnalyzer.build_insights(&entries, 60, 1_000, 3);

        assert_eq!(insights.severity_score, 100);
        assert_eq!(insights.error_rate_percent, 60);
        assert_eq!(insights.slow_rate_percent, 40);
        assert_eq!(insights.peak_window.as_ref().unwrap().entry_count, 4);
        assert_eq!(insights.peak_window.as_ref().unwrap().error_count, 2);
        assert_eq!(insights.correlations.len(), 2);
        assert_eq!(insights.correlations[0].key, "request_id=req-9001");
        assert_eq!(insights.correlations[0].error_count, 2);
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

    fn insight_mock_entries() -> Vec<LogEntry> {
        vec![
            insight_entry(LogLevel::Info, "api", "started request_id=req-9001", 0, 50),
            insight_entry(
                LogLevel::Error,
                "api",
                "database timeout request_id=req-9001",
                10,
                1_500,
            ),
            insight_entry(LogLevel::Warn, "worker", "retry job_id=job-77", 20, 1_100),
            insight_entry(
                LogLevel::Error,
                "api",
                "payment failed request_id=req-9001",
                40,
                900,
            ),
            insight_entry(
                LogLevel::Fatal,
                "worker",
                "worker crashed job_id=job-77",
                90,
                80,
            ),
        ]
    }

    fn insight_entry(
        level: LogLevel,
        source: &str,
        message: &str,
        second_offset: u32,
        duration_ms: u64,
    ) -> LogEntry {
        let message = format!("{message} duration_ms={duration_ms}");
        let timestamp = Utc.with_ymd_and_hms(2026, 6, 12, 12, 0, 0).unwrap()
            + Duration::seconds(second_offset.into());
        LogEntry {
            timestamp: LogTimestamp::new(timestamp),
            level,
            source: LogSource::new(source),
            fields: message
                .split_whitespace()
                .filter_map(|token| {
                    let (key, value) = token.split_once('=')?;
                    Some((key.to_string(), value.to_string()))
                })
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

/// Compact, owned summary shaped for frequently refreshed TUI panels.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealtimeSummary {
    pub total_count: usize,
    pub warning_count: usize,
    pub error_count: usize,
    pub top_sources: Vec<SourceRanking>,
    pub recent_lines: Vec<String>,
}

/// Higher-level operational signals used by reports and richer TUI panels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalInsights {
    pub severity_score: u8,
    pub error_rate_percent: u8,
    pub slow_rate_percent: u8,
    pub peak_window: Option<TimeWindowInsight>,
    pub correlations: Vec<CorrelationInsight>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindowInsight {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub entry_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationInsight {
    pub key: String,
    pub entry_count: usize,
    pub error_count: usize,
    pub sources: Vec<String>,
    pub sample_messages: Vec<String>,
}
