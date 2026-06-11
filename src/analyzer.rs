use crate::model::{LogEntry, LogLevel};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MODULE_NAME: &str = "analyzer";

const DEFAULT_TOP_KEYWORDS: usize = 10;
const DEFAULT_ERROR_SAMPLES: usize = 5;
const LEVEL_ORDER: [LogLevel; 6] = [
    LogLevel::Trace,
    LogLevel::Debug,
    LogLevel::Info,
    LogLevel::Warn,
    LogLevel::Error,
    LogLevel::Fatal,
];
const STOP_WORDS: &[&str] = &[
    "and", "are", "for", "from", "into", "not", "the", "that", "this", "was", "were", "while",
    "with",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogStats {
    pub total_lines: usize,
    pub level_counts: Vec<LevelCount>,
    pub error_ratio: f64,
    pub time_span: Option<TimeSpan>,
    pub top_keywords: Vec<KeywordCount>,
    pub error_samples: Vec<ErrorSample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelCount {
    pub level: LogLevel,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSpan {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub duration_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeywordCount {
    pub keyword: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorSample {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
    pub raw: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalyzerOptions {
    pub top_keyword_limit: usize,
    pub error_sample_limit: usize,
}

impl Default for AnalyzerOptions {
    fn default() -> Self {
        Self {
            top_keyword_limit: DEFAULT_TOP_KEYWORDS,
            error_sample_limit: DEFAULT_ERROR_SAMPLES,
        }
    }
}

pub fn analyze_entries(entries: &[LogEntry]) -> LogStats {
    analyze_entries_with_options(entries, AnalyzerOptions::default())
}

pub fn analyze_entries_with_options(entries: &[LogEntry], options: AnalyzerOptions) -> LogStats {
    let level_counts = count_levels(entries);
    let error_count = level_counts
        .iter()
        .filter(|count| count.level.severity() >= LogLevel::Error.severity())
        .map(|count| count.count)
        .sum::<usize>();

    LogStats {
        total_lines: entries.len(),
        level_counts,
        error_ratio: ratio(error_count, entries.len()),
        time_span: calculate_time_span(entries),
        top_keywords: top_keywords(entries, options.top_keyword_limit),
        error_samples: error_samples(entries, options.error_sample_limit),
    }
}

fn count_levels(entries: &[LogEntry]) -> Vec<LevelCount> {
    LEVEL_ORDER
        .into_iter()
        .map(|level| LevelCount {
            level,
            count: entries.iter().filter(|entry| entry.level == level).count(),
        })
        .collect()
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn calculate_time_span(entries: &[LogEntry]) -> Option<TimeSpan> {
    let start = entries.iter().map(|entry| entry.timestamp).min()?;
    let end = entries.iter().map(|entry| entry.timestamp).max()?;

    Some(TimeSpan {
        start,
        end,
        duration_seconds: (end - start).num_seconds(),
    })
}

fn top_keywords(entries: &[LogEntry], limit: usize) -> Vec<KeywordCount> {
    let mut counts = HashMap::new();

    for token in entries
        .iter()
        .flat_map(|entry| entry.message.split(|ch: char| !ch.is_alphanumeric()))
        .filter_map(normalize_keyword)
    {
        *counts.entry(token).or_insert(0usize) += 1;
    }

    let mut keywords = counts
        .into_iter()
        .map(|(keyword, count)| KeywordCount { keyword, count })
        .collect::<Vec<_>>();

    keywords.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.keyword.cmp(&right.keyword))
    });
    keywords.truncate(limit);
    keywords
}

fn normalize_keyword(token: &str) -> Option<String> {
    let keyword = token.to_ascii_lowercase();

    if keyword.len() < 3 || STOP_WORDS.contains(&keyword.as_str()) {
        None
    } else {
        Some(keyword)
    }
}

fn error_samples(entries: &[LogEntry], limit: usize) -> Vec<ErrorSample> {
    entries
        .iter()
        .filter(|entry| entry.level.severity() >= LogLevel::Error.severity())
        .take(limit)
        .map(|entry| ErrorSample {
            timestamp: entry.timestamp,
            level: entry.level,
            message: entry.message.clone(),
            raw: entry.raw.clone(),
        })
        .collect()
}
