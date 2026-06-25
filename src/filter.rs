//! Filter logic for narrowing log entries by level, source, keyword, and time range.
//!
//! The module exposes small predicates that can be composed from
//! [`FilterCondition`](crate::model::FilterCondition) and reused by CLI or UI flows.

/// Module identifier used for diagnostics and internal logging.
pub const MODULE_NAME: &str = "filter";

use crate::model::{FilterCondition, LogEntry, LogLevel, LogTimestamp, SearchResult};

/// Predicate interface used to compose log filters without cloning entries.
pub trait LogPredicate {
    fn matches(&self, entry: &LogEntry) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelFilter {
    level: LogLevel,
}

impl LevelFilter {
    pub const fn new(level: LogLevel) -> Self {
        Self { level }
    }
}

impl LogPredicate for LevelFilter {
    fn matches(&self, entry: &LogEntry) -> bool {
        entry.level == self.level
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordFilter {
    keyword: String,
}

impl KeywordFilter {
    pub fn new(keyword: impl Into<String>) -> Self {
        Self {
            keyword: keyword.into().to_lowercase(),
        }
    }
}

impl LogPredicate for KeywordFilter {
    fn matches(&self, entry: &LogEntry) -> bool {
        let haystack = entry.raw.to_lowercase();
        haystack.contains(&self.keyword)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFilter {
    source: String,
}

impl SourceFilter {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }
}

impl LogPredicate for SourceFilter {
    fn matches(&self, entry: &LogEntry) -> bool {
        entry.source.name == self.source
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeRangeFilter {
    start_time: Option<LogTimestamp>,
    end_time: Option<LogTimestamp>,
}

impl TimeRangeFilter {
    pub const fn new(start_time: Option<LogTimestamp>, end_time: Option<LogTimestamp>) -> Self {
        Self {
            start_time,
            end_time,
        }
    }
}

impl LogPredicate for TimeRangeFilter {
    fn matches(&self, entry: &LogEntry) -> bool {
        self.start_time
            .is_none_or(|start_time| entry.timestamp.value >= start_time.value)
            && self
                .end_time
                .is_none_or(|end_time| entry.timestamp.value <= end_time.value)
    }
}

#[derive(Default)]
pub struct CompositeFilter {
    predicates: Vec<Box<dyn LogPredicate>>,
}

impl CompositeFilter {
    pub fn from_condition(condition: &FilterCondition) -> Self {
        let mut filter = Self::default();

        if let Some(level) = condition.level {
            filter.push(LevelFilter::new(level));
        }
        if let Some(keyword) = condition
            .keyword
            .as_deref()
            .filter(|keyword| !keyword.is_empty())
        {
            filter.push(KeywordFilter::new(keyword));
        }
        if let Some(source) = condition
            .source
            .as_deref()
            .filter(|source| !source.is_empty())
        {
            filter.push(SourceFilter::new(source));
        }
        if condition.start_time.is_some() || condition.end_time.is_some() {
            filter.push(TimeRangeFilter::new(
                condition.start_time,
                condition.end_time,
            ));
        }

        filter
    }

    pub fn push(&mut self, predicate: impl LogPredicate + 'static) {
        self.predicates.push(Box::new(predicate));
    }
}

impl LogPredicate for CompositeFilter {
    fn matches(&self, entry: &LogEntry) -> bool {
        self.predicates
            .iter()
            .all(|predicate| predicate.matches(entry))
    }
}

pub fn filter_entries<'a>(
    entries: &'a [LogEntry],
    condition: &FilterCondition,
) -> SearchResult<'a> {
    let filter = CompositeFilter::from_condition(condition);
    SearchResult::new(
        entries
            .iter()
            .filter(|entry| filter.matches(entry))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        CompositeFilter, KeywordFilter, LevelFilter, LogPredicate, SourceFilter, TimeRangeFilter,
        filter_entries,
    };
    use crate::model::{FilterCondition, LogEntry, LogLevel, LogSource, LogTimestamp};
    use chrono::{TimeZone, Utc};

    #[test]
    fn filters_by_level_keyword_source_and_time_range() {
        let entries = sample_entries();
        let condition = FilterCondition {
            keyword: Some("timeout".to_string()),
            level: Some(LogLevel::Error),
            source: Some("api".to_string()),
            start_time: Some(timestamp(10, 1)),
            end_time: Some(timestamp(10, 3)),
        };

        let matches = filter_entries(&entries, &condition);

        assert_eq!(matches.total_matches, 1);
        assert_eq!(matches.entries[0].message, "database timeout");
    }

    #[test]
    fn empty_condition_matches_all_entries() {
        let entries = sample_entries();
        let matches = filter_entries(&entries, &FilterCondition::default());

        assert_eq!(matches.total_matches, 4);
    }

    #[test]
    fn predicates_can_be_composed_directly() {
        let entries = sample_entries();
        let mut filter = CompositeFilter::default();
        filter.push(LevelFilter::new(LogLevel::Warn));
        filter.push(KeywordFilter::new("retry"));
        filter.push(SourceFilter::new("worker"));

        let matches = entries
            .iter()
            .filter(|entry| filter.matches(entry))
            .collect::<Vec<_>>();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message, "retrying job");
    }

    #[test]
    fn time_range_filter_is_inclusive() {
        let entries = sample_entries();
        let filter = TimeRangeFilter::new(Some(timestamp(10, 1)), Some(timestamp(10, 2)));
        let matches = entries
            .iter()
            .filter(|entry| filter.matches(entry))
            .collect::<Vec<_>>();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].level, LogLevel::Warn);
        assert_eq!(matches[1].level, LogLevel::Error);
    }

    fn sample_entries() -> Vec<LogEntry> {
        vec![
            entry(LogLevel::Info, "api", "started", 0),
            entry(LogLevel::Warn, "worker", "retrying job", 1),
            entry(LogLevel::Error, "api", "database timeout", 2),
            entry(LogLevel::Fatal, "worker", "worker crashed", 3),
        ]
    }

    fn entry(level: LogLevel, source: &str, message: &str, minute: u32) -> LogEntry {
        let timestamp = timestamp(10, minute);
        LogEntry {
            timestamp,
            level,
            source: LogSource::new(source),
            message: message.to_string(),
            fields: Default::default(),
            raw: format!(
                "{} {} {} {}",
                timestamp.value.to_rfc3339(),
                level,
                source,
                message
            ),
        }
    }

    fn timestamp(hour: u32, minute: u32) -> LogTimestamp {
        LogTimestamp::new(Utc.with_ymd_and_hms(2026, 6, 12, hour, minute, 0).unwrap())
    }
}
