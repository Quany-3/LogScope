use crate::model::{LogEntry, LogLevel};
use chrono::{DateTime, Utc};
use std::collections::HashSet;

pub const MODULE_NAME: &str = "filter";

pub trait LogFilter {
    fn matches(&self, entry: &LogEntry) -> bool;
}

#[derive(Debug, Clone, Default)]
pub struct LevelFilter {
    levels: HashSet<LogLevel>,
}

impl LevelFilter {
    pub fn new(levels: impl IntoIterator<Item = LogLevel>) -> Self {
        Self {
            levels: levels.into_iter().collect(),
        }
    }

    pub fn single(level: LogLevel) -> Self {
        Self::new([level])
    }
}

impl LogFilter for LevelFilter {
    fn matches(&self, entry: &LogEntry) -> bool {
        self.levels.is_empty() || self.levels.contains(&entry.level)
    }
}

#[derive(Debug, Clone)]
pub struct KeywordFilter {
    keyword: String,
    case_sensitive: bool,
}

impl KeywordFilter {
    pub fn new(keyword: impl Into<String>) -> Self {
        Self {
            keyword: keyword.into(),
            case_sensitive: false,
        }
    }

    pub fn case_sensitive(keyword: impl Into<String>) -> Self {
        Self {
            keyword: keyword.into(),
            case_sensitive: true,
        }
    }
}

impl LogFilter for KeywordFilter {
    fn matches(&self, entry: &LogEntry) -> bool {
        if self.keyword.is_empty() {
            return true;
        }

        if self.case_sensitive {
            entry.message.contains(&self.keyword) || entry.raw.contains(&self.keyword)
        } else {
            let keyword = self.keyword.to_ascii_lowercase();
            entry.message.to_ascii_lowercase().contains(&keyword)
                || entry.raw.to_ascii_lowercase().contains(&keyword)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TimeRangeFilter {
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

impl TimeRangeFilter {
    pub fn new(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> Self {
        Self { start, end }
    }

    pub fn after(start: DateTime<Utc>) -> Self {
        Self::new(Some(start), None)
    }

    pub fn before(end: DateTime<Utc>) -> Self {
        Self::new(None, Some(end))
    }
}

impl LogFilter for TimeRangeFilter {
    fn matches(&self, entry: &LogEntry) -> bool {
        if self.start.is_some_and(|start| entry.timestamp < start) {
            return false;
        }

        if self.end.is_some_and(|end| entry.timestamp > end) {
            return false;
        }

        true
    }
}

#[derive(Default)]
pub struct FilterChain {
    filters: Vec<Box<dyn LogFilter>>,
}

impl FilterChain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with<F>(mut self, filter: F) -> Self
    where
        F: LogFilter + 'static,
    {
        self.push(filter);
        self
    }

    pub fn push<F>(&mut self, filter: F)
    where
        F: LogFilter + 'static,
    {
        self.filters.push(Box::new(filter));
    }

    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }
}

impl LogFilter for FilterChain {
    fn matches(&self, entry: &LogEntry) -> bool {
        self.filters.iter().all(|filter| filter.matches(entry))
    }
}

pub fn filter_entries<'a, F>(
    entries: impl IntoIterator<Item = &'a LogEntry>,
    filter: &F,
) -> Vec<&'a LogEntry>
where
    F: LogFilter + ?Sized,
{
    entries
        .into_iter()
        .filter(|entry| filter.matches(entry))
        .collect()
}
