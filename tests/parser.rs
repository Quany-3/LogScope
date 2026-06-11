use log_scope::{
    model::LogLevel,
    parser::{ParseError, parse_line},
};

fn parse_ok(line: &str) -> log_scope::model::LogEntry {
    match parse_line(line) {
        Ok(entry) => entry,
        Err(error) => panic!("expected `{line}` to parse, got {error}"),
    }
}

#[test]
fn parses_bracketed_datetime_log_line() {
    let raw = "[2026-06-11 12:00:00] INFO service started";
    let entry = parse_ok(raw);

    assert_eq!(entry.timestamp.to_rfc3339(), "2026-06-11T12:00:00+00:00");
    assert_eq!(entry.level, LogLevel::Info);
    assert_eq!(entry.message, "service started");
    assert_eq!(entry.raw, raw);
}

#[test]
fn parses_rfc3339_datetime_log_line() {
    let raw = "2026-06-11T12:00:00Z ERROR request failed";
    let entry = parse_ok(raw);

    assert_eq!(entry.timestamp.to_rfc3339(), "2026-06-11T12:00:00+00:00");
    assert_eq!(entry.level, LogLevel::Error);
    assert_eq!(entry.message, "request failed");
    assert_eq!(entry.raw, raw);
}

#[test]
fn rejects_unknown_level_with_raw_line() {
    let raw = "[2026-06-11 12:00:00] NOTICE cache warmed";

    match parse_line(raw) {
        Err(ParseError::UnknownLevel { level, raw: line }) => {
            assert_eq!(level, "NOTICE");
            assert_eq!(line, raw);
        }
        other => panic!("expected unknown level error, got {other:?}"),
    }
}

#[test]
fn rejects_bad_timestamp_with_context() {
    let raw = "[2026-99-11 12:00:00] INFO impossible date";

    match parse_line(raw) {
        Err(ParseError::InvalidTimestamp {
            timestamp,
            raw: line,
            reason,
        }) => {
            assert_eq!(timestamp, "2026-99-11 12:00:00");
            assert_eq!(line, raw);
            assert!(!reason.is_empty());
        }
        other => panic!("expected invalid timestamp error, got {other:?}"),
    }
}

#[test]
fn rejects_empty_lines() {
    match parse_line("   ") {
        Err(ParseError::EmptyLine { raw }) => assert_eq!(raw, "   "),
        other => panic!("expected empty line error, got {other:?}"),
    }
}
