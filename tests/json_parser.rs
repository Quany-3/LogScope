use log_scope::model::LogLevel;
use log_scope::parser::{JsonLineLogParser, LogParser, ParseError};

#[test]
fn parses_json_parser_sample_lines() {
    let parser = JsonLineLogParser;
    let entries = include_str!("../samples/json.log")
        .lines()
        .map(|line| parser.parse_line(line).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].level, LogLevel::Info);
    assert_eq!(entries[1].level, LogLevel::Error);
    assert_eq!(entries[1].source.name, "worker");
}

#[test]
fn rejects_invalid_json_parser_formats() {
    let parser = JsonLineLogParser;

    let invalid_json = parser.parse_line("{not-json}");
    let missing_message =
        parser.parse_line(r#"{"timestamp":"2026-06-12T10:00:00Z","level":"INFO","source":"api"}"#);

    assert!(matches!(
        invalid_json.unwrap_err(),
        ParseError::InvalidJson { .. }
    ));
    assert!(matches!(
        missing_message.unwrap_err(),
        ParseError::MissingField { field } if field == "message"
    ));
}
