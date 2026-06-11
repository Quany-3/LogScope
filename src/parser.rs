use crate::model::{LogEntry, LogLevelParseError};
use chrono::{DateTime, NaiveDateTime, Utc};
use thiserror::Error;

pub const MODULE_NAME: &str = "parser";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("empty log line")]
    EmptyLine { raw: String },
    #[error("log line does not match a supported format: `{raw}`")]
    UnsupportedFormat { raw: String },
    #[error("invalid timestamp `{timestamp}` in line `{raw}`: {reason}")]
    InvalidTimestamp {
        timestamp: String,
        raw: String,
        reason: String,
    },
    #[error("missing log level in line `{raw}`")]
    MissingLevel { raw: String },
    #[error("unknown log level `{level}` in line `{raw}`")]
    UnknownLevel { level: String, raw: String },
    #[error("missing log message in line `{raw}`")]
    MissingMessage { raw: String },
}

pub fn parse_line(line: &str) -> Result<LogEntry, ParseError> {
    let raw = line.to_string();
    let trimmed = line.trim();

    if trimmed.is_empty() {
        return Err(ParseError::EmptyLine { raw });
    }

    if let Some(without_opening) = trimmed.strip_prefix('[') {
        return parse_bracketed_datetime(without_opening, raw);
    }

    parse_rfc3339_prefix(trimmed, raw)
}

fn parse_bracketed_datetime(line: &str, raw: String) -> Result<LogEntry, ParseError> {
    let Some((timestamp_text, rest)) = line.split_once(']') else {
        return Err(ParseError::UnsupportedFormat { raw });
    };

    let timestamp = NaiveDateTime::parse_from_str(timestamp_text.trim(), "%Y-%m-%d %H:%M:%S")
        .map(|value| value.and_utc())
        .map_err(|error| ParseError::InvalidTimestamp {
            timestamp: timestamp_text.trim().to_string(),
            raw: raw.clone(),
            reason: error.to_string(),
        })?;

    parse_payload(timestamp, rest.trim_start(), raw)
}

fn parse_rfc3339_prefix(line: &str, raw: String) -> Result<LogEntry, ParseError> {
    let Some((timestamp_text, rest)) = split_first_token(line) else {
        return Err(ParseError::UnsupportedFormat { raw });
    };

    let timestamp = DateTime::parse_from_rfc3339(timestamp_text)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| ParseError::InvalidTimestamp {
            timestamp: timestamp_text.to_string(),
            raw: raw.clone(),
            reason: error.to_string(),
        })?;

    parse_payload(timestamp, rest.trim_start(), raw)
}

fn parse_payload(
    timestamp: DateTime<Utc>,
    payload: &str,
    raw: String,
) -> Result<LogEntry, ParseError> {
    if payload.is_empty() {
        return Err(ParseError::MissingLevel { raw });
    }

    let Some((level_text, message)) = split_first_token(payload) else {
        return Err(ParseError::MissingMessage { raw });
    };

    let level =
        level_text
            .parse()
            .map_err(|error: LogLevelParseError| ParseError::UnknownLevel {
                level: error.token,
                raw: raw.clone(),
            })?;

    let message = message.trim_start();
    if message.is_empty() {
        return Err(ParseError::MissingMessage { raw });
    }

    Ok(LogEntry::new(timestamp, level, message, raw))
}

fn split_first_token(value: &str) -> Option<(&str, &str)> {
    let value = value.trim_start();
    let split_at = value.find(char::is_whitespace)?;
    Some((&value[..split_at], value[split_at..].trim_start()))
}
