use log_scope::model::LogLevel;
use log_scope::parser::{LogParser, ParseError, PlainTextLogParser};

#[test]
fn parses_text_parser_sample_lines() {
    let parser = PlainTextLogParser;
    let entries = include_str!("../samples/plain.log")
        .lines()
        .map(|line| parser.parse_line(line).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].level, LogLevel::Info);
    assert_eq!(entries[1].level, LogLevel::Warn);
    assert_eq!(entries[2].level, LogLevel::Error);
    assert_eq!(entries[2].message, "database timeout");
}

#[test]
fn rejects_invalid_text_parser_formats() {
    let parser = PlainTextLogParser;

    let missing_message = parser.parse_line("2026-06-12T10:00:00Z INFO api");
    let invalid_timestamp = parser.parse_line("invalid-time INFO api request completed");
    let invalid_level = parser.parse_line("2026-06-12T10:00:00Z NOTICE api request completed");

    assert!(matches!(
        missing_message.unwrap_err(),
        ParseError::InvalidFormat { .. }
    ));
    assert!(matches!(
        invalid_timestamp.unwrap_err(),
        ParseError::InvalidTimestamp { .. }
    ));
    assert!(matches!(
        invalid_level.unwrap_err(),
        ParseError::InvalidLevel { .. }
    ));
}
