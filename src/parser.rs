pub const MODULE_NAME: &str = "parser";

use crate::model::{LogEntry, LogLevel, LogSource, LogTimestamp};
use chrono::{DateTime, Utc};
use serde::Deserialize;
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

/// Parser for one JSON object per line with timestamp, level, source, and message fields.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonLineLogParser;

impl LogParser for JsonLineLogParser {
    fn parse_line(&self, line: &str) -> ParseResult<LogEntry> {
        let parsed: JsonLogLine =
            serde_json::from_str(line).map_err(|_| ParseError::InvalidJson {
                line: line.to_string(),
            })?;

        let timestamp = required_field(parsed.timestamp, "timestamp")?;
        let level = required_field(parsed.level, "level")?;
        let source = required_field(parsed.source, "source")?;
        let message = required_field(parsed.message, "message")?;

        Ok(LogEntry {
            timestamp: LogTimestamp::new(parse_timestamp(&timestamp)?),
            level: parse_level(&level)?,
            source: LogSource::new(source),
            message,
            raw: line.to_string(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct JsonLogLine {
    timestamp: Option<String>,
    level: Option<String>,
    source: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("invalid log line format: {line}")]
    InvalidFormat { line: String },
    #[error("invalid timestamp: {value}")]
    InvalidTimestamp { value: String },
    #[error("invalid log level: {value}")]
    InvalidLevel { value: String },
    #[error("invalid json log line: {line}")]
    InvalidJson { line: String },
    #[error("missing required field: {field}")]
    MissingField { field: &'static str },
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
    LogLevel::from_label(value).ok_or_else(|| ParseError::InvalidLevel {
        value: value.to_string(),
    })
}

fn required_field(value: Option<String>, field: &'static str) -> ParseResult<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or(ParseError::MissingField { field })
}

#[cfg(test)]
mod tests {
    use super::{JsonLineLogParser, LogParser, ParseError, PlainTextLogParser};
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

    #[test]
    fn parses_json_line_log() {
        let parser = JsonLineLogParser;
        let entry = parser
            .parse_line(
                r#"{"timestamp":"2026-06-12T10:02:00Z","level":"ERROR","source":"worker","message":"database timeout"}"#,
            )
            .unwrap();

        assert_eq!(
            entry.timestamp.value,
            Utc.with_ymd_and_hms(2026, 6, 12, 10, 2, 0).unwrap()
        );
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.source.name, "worker");
        assert_eq!(entry.message, "database timeout");
    }

    #[test]
    fn rejects_json_line_with_missing_field() {
        let parser = JsonLineLogParser;
        let error = parser
            .parse_line(r#"{"timestamp":"2026-06-12T10:02:00Z","level":"ERROR","source":"worker"}"#)
            .unwrap_err();

        assert!(matches!(error, ParseError::MissingField { field } if field == "message"));
    }

    #[test]
    fn normalizes_parsed_log_levels() {
        let text_entry = PlainTextLogParser
            .parse_line("2026-06-12T10:02:00Z warning api retrying")
            .unwrap();
        let json_entry = JsonLineLogParser
            .parse_line(
                r#"{"timestamp":"2026-06-12T10:02:00Z","level":"error","source":"api","message":"failed"}"#,
            )
            .unwrap();

        assert_eq!(text_entry.level, LogLevel::Warn);
        assert_eq!(json_entry.level, LogLevel::Error);
    }

    #[test]
    fn preserves_raw_line_for_all_parsers() {
        let text = "2026-06-12T10:02:00Z ERROR api database timeout";
        let json = r#"{"timestamp":"2026-06-12T10:02:00Z","level":"ERROR","source":"api","message":"database timeout"}"#;

        let text_entry = PlainTextLogParser.parse_line(text).unwrap();
        let json_entry = JsonLineLogParser.parse_line(json).unwrap();

        assert_eq!(text_entry.raw, text);
        assert_eq!(json_entry.raw, json);
    }
}
