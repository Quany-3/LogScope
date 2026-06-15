pub const MODULE_NAME: &str = "parser";

use crate::model::{LogEntry, LogLevel, LogSource, LogTimestamp};
use chrono::{DateTime, Utc};
use thiserror::Error;

pub type ParseResult<T> = Result<T, ParseError>;

/// Common interface for line-oriented log parsers.
pub trait LogParser {
    fn parse_line(&self, line: &str) -> ParseResult<LogEntry>;
}

/// Parser for lines shaped as: timestamp level source message.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlainTextLogParser;

impl LogParser for PlainTextLogParser {
    fn parse_line(&self, line: &str) -> ParseResult<LogEntry> {
        let mut parts = line.splitn(4, char::is_whitespace);
        let timestamp = parts
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ParseError::invalid_format(line))?;
        let level = parts
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ParseError::invalid_format(line))?;
        let source = parts
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ParseError::invalid_format(line))?;
        let message = parts
            .next()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ParseError::invalid_format(line))?;

        Ok(LogEntry {
            timestamp: LogTimestamp::new(parse_timestamp(timestamp)?),
            level: parse_level(level)?,
            source: LogSource::new(source),
            message: message.to_string(),
            raw: line.to_string(),
        })
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("invalid log line format: {line}")]
    InvalidFormat { line: String },
    #[error("invalid timestamp: {value}")]
    InvalidTimestamp { value: String },
    #[error("invalid log level: {value}")]
    InvalidLevel { value: String },
}

impl ParseError {
    fn invalid_format(line: &str) -> Self {
        Self::InvalidFormat {
            line: line.to_string(),
        }
    }
}

fn parse_timestamp(value: &str) -> ParseResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| ParseError::InvalidTimestamp {
            value: value.to_string(),
        })
}

fn parse_level(value: &str) -> ParseResult<LogLevel> {
    match value.to_ascii_uppercase().as_str() {
        "TRACE" => Ok(LogLevel::Trace),
        "DEBUG" => Ok(LogLevel::Debug),
        "INFO" => Ok(LogLevel::Info),
        "WARN" | "WARNING" => Ok(LogLevel::Warn),
        "ERROR" => Ok(LogLevel::Error),
        "FATAL" => Ok(LogLevel::Fatal),
        _ => Err(ParseError::InvalidLevel {
            value: value.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{LogParser, ParseError, PlainTextLogParser};
    use crate::model::LogLevel;
    use chrono::{TimeZone, Utc};

    #[test]
    fn parses_plain_text_log_line() {
        let parser = PlainTextLogParser;
        let entry = parser
            .parse_line("2026-06-12T10:02:00Z ERROR api database timeout")
            .unwrap();

        assert_eq!(
            entry.timestamp.value,
            Utc.with_ymd_and_hms(2026, 6, 12, 10, 2, 0).unwrap()
        );
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.source.name, "api");
        assert_eq!(entry.message, "database timeout");
        assert_eq!(entry.raw, "2026-06-12T10:02:00Z ERROR api database timeout");
    }

    #[test]
    fn rejects_invalid_plain_text_log_line() {
        let parser = PlainTextLogParser;
        let error = parser.parse_line("2026-06-12T10:02:00Z ERROR").unwrap_err();

        assert!(matches!(error, ParseError::InvalidFormat { .. }));
    }

    #[test]
    fn rejects_invalid_timestamp() {
        let parser = PlainTextLogParser;
        let error = parser
            .parse_line("not-a-time ERROR api database timeout")
            .unwrap_err();

        assert!(matches!(error, ParseError::InvalidTimestamp { .. }));
    }

    #[test]
    fn rejects_invalid_level() {
        let parser = PlainTextLogParser;
        let error = parser
            .parse_line("2026-06-12T10:02:00Z NOTICE api database timeout")
            .unwrap_err();

        assert!(matches!(error, ParseError::InvalidLevel { .. }));
    }
}
