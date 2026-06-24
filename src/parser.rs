pub const MODULE_NAME: &str = "parser";

use crate::model::{LogEntry, LogLevel, LogSource, LogTimestamp};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub type ParseResult<T> = Result<T, ParseError>;

/// Common interface for line-oriented log parsers.
pub trait LogParser {
    fn parse_line(&self, line: &str) -> ParseResult<LogEntry>;
}

/// Parse a complete log file with a caller-selected line parser.
pub fn parse_file(
    path: impl AsRef<Path>,
    parser: &dyn LogParser,
) -> Result<Vec<LogEntry>, ParseFileError> {
    let mut entries = Vec::new();
    parse_file_with(path, parser, |entry| {
        entries.push(entry);
    })?;
    Ok(entries)
}

/// Stream parsed entries to the caller without building an intermediate list.
pub fn parse_file_with(
    path: impl AsRef<Path>,
    parser: &dyn LogParser,
    mut visitor: impl FnMut(LogEntry),
) -> Result<(), ParseFileError> {
    let path = path.as_ref().to_path_buf();
    let file = File::open(&path).map_err(|source| ParseFileError::Open {
        path: path.clone(),
        source,
    })?;

    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line_number = index + 1;
        let line = line.map_err(|source| ParseFileError::ReadLine {
            path: path.clone(),
            line_number,
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }

        let entry = parser
            .parse_line(&line)
            .map_err(|source| ParseFileError::ParseLine {
                path: path.clone(),
                line_number,
                source,
            })?;
        visitor(entry);
    }

    Ok(())
}

/// Parse a log file after detecting whether its first non-empty line is JSON.
pub fn parse_file_auto(path: impl AsRef<Path>) -> Result<Vec<LogEntry>, ParseFileError> {
    let path = path.as_ref();
    if should_parse_as_json(path)? {
        parse_file(path, &JsonLineLogParser)
    } else {
        parse_file(path, &PlainTextLogParser)
    }
}

/// Stream a log file after detecting whether its first non-empty line is JSON.
pub fn parse_file_auto_with(
    path: impl AsRef<Path>,
    visitor: impl FnMut(LogEntry),
) -> Result<(), ParseFileError> {
    let path = path.as_ref();
    if should_parse_as_json(path)? {
        parse_file_with(path, &JsonLineLogParser, visitor)
    } else {
        parse_file_with(path, &PlainTextLogParser, visitor)
    }
}

fn should_parse_as_json(path: &Path) -> Result<bool, ParseFileError> {
    if matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("json" | "jsonl")
    ) {
        return Ok(true);
    }

    let path = path.to_path_buf();
    let file = File::open(&path).map_err(|source| ParseFileError::Open {
        path: path.clone(),
        source,
    })?;
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line_number = index + 1;
        let line = line.map_err(|source| ParseFileError::ReadLine {
            path: path.clone(),
            line_number,
            source,
        })?;
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        return Ok(trimmed.starts_with('{'));
    }

    Ok(false)
}

/// Parser for lines shaped as: timestamp level source message.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlainTextLogParser;

impl LogParser for PlainTextLogParser {
    fn parse_line(&self, line: &str) -> ParseResult<LogEntry> {
        if let Some(entry) = parse_spring_logback_line(line)? {
            return Ok(entry);
        }

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
        let fields = extract_structured_fields(message);

        Ok(LogEntry {
            timestamp: LogTimestamp::new(parse_timestamp(timestamp)?),
            level: parse_level(level)?,
            source: LogSource::new(source),
            message: message.to_string(),
            fields,
            raw: line.to_string(),
        })
    }
}

fn parse_spring_logback_line(line: &str) -> ParseResult<Option<LogEntry>> {
    let Some((time, rest)) = line.split_once(char::is_whitespace) else {
        return Ok(None);
    };
    let Ok(parsed_time) = NaiveTime::parse_from_str(time, "%H:%M:%S%.f") else {
        return Ok(None);
    };

    let rest = rest.trim_start();
    if !rest.starts_with('[') {
        return Ok(None);
    }
    let Some(thread_end) = rest.find(']') else {
        return Err(ParseError::invalid_format(line));
    };
    let thread = &rest[1..thread_end];
    let after_thread = rest[thread_end + 1..].trim_start();
    let (level, after_level) = after_thread
        .split_once(char::is_whitespace)
        .ok_or_else(|| ParseError::invalid_format(line))?;
    let level = parse_level(level)?;
    let (source, after_source) = after_level
        .trim_start()
        .split_once(" - ")
        .ok_or_else(|| ParseError::invalid_format(line))?;
    let (caller, message) = parse_logback_caller_and_message(after_source);
    let mut fields = extract_structured_fields(message);
    fields.insert("thread".to_string(), thread.to_string());
    fields.insert("display_timestamp".to_string(), time.to_string());
    if let Some(caller) = caller {
        fields.insert("caller".to_string(), caller.to_string());
    }

    Ok(Some(LogEntry {
        // Date-less logback rows keep a synthetic date internally for sorting.
        timestamp: LogTimestamp::new(DateTime::from_naive_utc_and_offset(
            NaiveDate::from_ymd_opt(1970, 1, 1)
                .expect("valid synthetic date")
                .and_time(parsed_time),
            Utc,
        )),
        level,
        source: LogSource::new(source.trim()),
        message: message.to_string(),
        fields,
        raw: line.to_string(),
    }))
}

fn parse_logback_caller_and_message(value: &str) -> (Option<&str>, &str) {
    let value = value.trim_start();
    if !value.starts_with('[') {
        return (None, value);
    }

    let Some(caller_end) = value.find(']') else {
        return (None, value);
    };
    let caller = &value[1..caller_end];
    let after_caller = value[caller_end + 1..].trim_start();
    let message = after_caller
        .strip_prefix("- ")
        .map(str::trim_start)
        .unwrap_or(after_caller);

    (Some(caller), message)
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
        let fields = extract_structured_fields(&message);

        Ok(LogEntry {
            timestamp: LogTimestamp::new(parse_timestamp(&timestamp)?),
            level: parse_level(&level)?,
            source: LogSource::new(source),
            message,
            fields,
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

#[derive(Debug, Error)]
pub enum ParseFileError {
    #[error("failed to open log file {path}", path = .path.display())]
    Open {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "failed to read line {line_number} from log file {path}",
        path = .path.display()
    )]
    ReadLine {
        path: PathBuf,
        line_number: usize,
        #[source]
        source: io::Error,
    },
    #[error(
        "failed to parse line {line_number} in log file {path}: {source}",
        path = .path.display()
    )]
    ParseLine {
        path: PathBuf,
        line_number: usize,
        #[source]
        source: ParseError,
    },
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
    use super::{
        JsonLineLogParser, LogParser, ParseError, ParseFileError, PlainTextLogParser, parse_file,
        parse_file_auto, parse_file_auto_with, parse_file_with,
    };
    use crate::model::LogLevel;
    use chrono::{TimeZone, Utc};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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

    #[test]
    fn extracts_structured_fields_from_messages() {
        let text_entry = PlainTextLogParser
            .parse_line("2026-06-12T10:02:00Z ERROR api request_failed status=500 duration_ms=125")
            .unwrap();
        let json_entry = JsonLineLogParser
            .parse_line(
                r#"{"timestamp":"2026-06-12T10:02:00Z","level":"ERROR","source":"api","message":"request_failed status=503 retry=true"}"#,
            )
            .unwrap();

        assert_eq!(text_entry.fields["status"], "500");
        assert_eq!(text_entry.fields["duration_ms"], "125");
        assert_eq!(json_entry.fields["status"], "503");
        assert_eq!(json_entry.fields["retry"], "true");
    }

    #[test]
    fn parses_text_file_and_skips_blank_lines() {
        let path = write_temp_log(
            "2026-06-12T10:00:00Z INFO api started\n\n2026-06-12T10:01:00Z ERROR api failed\n",
        );

        let entries = parse_file(&path, &PlainTextLogParser).unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].level, LogLevel::Error);
    }

    #[test]
    fn parses_json_file_with_the_shared_helper() {
        let path = write_temp_log(
            r#"{"timestamp":"2026-06-12T10:00:00Z","level":"INFO","source":"api","message":"started"}
"#,
        );

        let entries = parse_file(&path, &JsonLineLogParser).unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source.name, "api");
    }

    #[test]
    fn parses_spring_logback_line_without_displaying_a_date() {
        let parser = PlainTextLogParser;
        let entry = parser
            .parse_line(
                "15:19:18.955 [background-preinit] INFO  o.h.v.i.util.Version - [<clinit>,21] - HV000001: Hibernate Validator 8.0.2.Final",
            )
            .unwrap();

        assert_eq!(entry.display_timestamp(), "15:19:18.955");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.source.name, "o.h.v.i.util.Version");
        assert_eq!(entry.message, "HV000001: Hibernate Validator 8.0.2.Final");
        assert_eq!(entry.fields["thread"], "background-preinit");
        assert_eq!(entry.fields["caller"], "<clinit>,21");
    }

    #[test]
    fn streams_parsed_entries_without_returning_collection() {
        let path = write_temp_log(
            "2026-06-12T10:00:00Z INFO api started\n2026-06-12T10:01:00Z ERROR api failed\n",
        );
        let mut levels = Vec::new();

        parse_file_with(&path, &PlainTextLogParser, |entry| {
            levels.push(entry.level);
        })
        .unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(levels, vec![LogLevel::Info, LogLevel::Error]);
    }

    #[test]
    fn auto_parser_detects_json_content_with_log_extension() {
        let path = write_temp_log(
            r#"{"timestamp":"2026-06-12T10:00:00Z","level":"INFO","source":"api","message":"started"}
"#,
        );

        let entries = parse_file_auto(&path).unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source.name, "api");
    }

    #[test]
    fn auto_parser_keeps_plain_text_logs_as_text() {
        let path = write_temp_log("2026-06-12T10:00:00Z WARN api retrying\n");

        let entries = parse_file_auto(&path).unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, LogLevel::Warn);
    }

    #[test]
    fn auto_parser_streams_json_logs() {
        let path = write_temp_log(
            r#"{"timestamp":"2026-06-12T10:00:00Z","level":"WARN","source":"api","message":"retrying"}
"#,
        );
        let mut sources = Vec::new();

        parse_file_auto_with(&path, |entry| {
            sources.push(entry.source.name);
        })
        .unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(sources, vec!["api"]);
    }

    #[test]
    fn reports_the_physical_line_number_for_invalid_entries() {
        let path = write_temp_log(
            "2026-06-12T10:00:00Z INFO api started\n\n2026-06-12T10:01:00Z NOTICE api failed\n",
        );

        let error = parse_file(&path, &PlainTextLogParser).unwrap_err();

        fs::remove_file(path).unwrap();
        assert!(matches!(
            error,
            ParseFileError::ParseLine {
                line_number: 3,
                source: ParseError::InvalidLevel { .. },
                ..
            }
        ));
    }

    fn write_temp_log(content: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("logscope-parser-{suffix}.log"));
        fs::write(&path, content).unwrap();
        path
    }
}

/// Extract whitespace-delimited key=value tokens while preserving the message.
fn extract_structured_fields(message: &str) -> BTreeMap<String, String> {
    message
        .split_whitespace()
        .filter_map(|token| {
            let (key, value) = token.split_once('=')?;
            if key.is_empty() || value.is_empty() {
                return None;
            }
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}
