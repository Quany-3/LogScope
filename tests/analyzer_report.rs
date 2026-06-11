use log_scope::{
    analyzer::{AnalyzerOptions, analyze_entries_with_options},
    model::{LogEntry, LogLevel},
    parser::parse_line,
    report::{ReportFormat, render_report, render_text},
};
use serde_json::Value;

fn entry(line: &str) -> LogEntry {
    match parse_line(line) {
        Ok(entry) => entry,
        Err(error) => panic!("expected `{line}` to parse, got {error}"),
    }
}

fn sample_entries() -> Vec<LogEntry> {
    vec![
        entry("[2026-06-11 12:00:00] INFO service started"),
        entry("[2026-06-11 12:01:00] WARN cache miss for user 42"),
        entry("[2026-06-11 12:02:00] ERROR database timeout"),
        entry("[2026-06-11 12:02:30] ERROR database retry failed"),
        entry("[2026-06-11 12:03:00] FATAL database unavailable"),
    ]
}

#[test]
fn analyzes_log_statistics() {
    let entries = sample_entries();
    let stats = analyze_entries_with_options(
        &entries,
        AnalyzerOptions {
            top_keyword_limit: 3,
            error_sample_limit: 2,
        },
    );

    assert_eq!(stats.total_lines, 5);
    assert_eq!(level_count(&stats, LogLevel::Info), 1);
    assert_eq!(level_count(&stats, LogLevel::Warn), 1);
    assert_eq!(level_count(&stats, LogLevel::Error), 2);
    assert_eq!(level_count(&stats, LogLevel::Fatal), 1);
    assert!((stats.error_ratio - 0.6).abs() < f64::EPSILON);

    let span = stats.time_span.expect("time span");
    assert_eq!(span.start.to_rfc3339(), "2026-06-11T12:00:00+00:00");
    assert_eq!(span.end.to_rfc3339(), "2026-06-11T12:03:00+00:00");
    assert_eq!(span.duration_seconds, 180);

    assert_eq!(stats.top_keywords[0].keyword, "database");
    assert_eq!(stats.top_keywords[0].count, 3);
    assert_eq!(stats.error_samples.len(), 2);
    assert_eq!(stats.error_samples[0].message, "database timeout");
}

#[test]
fn renders_text_report_with_summary_sections() {
    let entries = sample_entries();
    let stats = analyze_entries_with_options(&entries, AnalyzerOptions::default());
    let report = render_text(&stats);

    assert!(report.contains("Total lines: 5"));
    assert!(report.contains("Error ratio: 60.00%"));
    assert!(report.contains("ERROR: 2"));
    assert!(report.contains("database: 3"));
    assert!(report.contains("Error samples:"));
}

#[test]
fn renders_parseable_json_report() {
    let entries = sample_entries();
    let stats = analyze_entries_with_options(&entries, AnalyzerOptions::default());
    let report = render_report(&stats, ReportFormat::Json).expect("json report");
    let parsed: Value = serde_json::from_str(&report).expect("parse json report");

    assert_eq!(parsed["total_lines"], 5);
    assert_eq!(parsed["level_counts"][4]["level"], "error");
    assert_eq!(parsed["level_counts"][4]["count"], 2);
    assert_eq!(parsed["top_keywords"][0]["keyword"], "database");
}

fn level_count(stats: &log_scope::analyzer::LogStats, level: LogLevel) -> usize {
    stats
        .level_counts
        .iter()
        .find(|count| count.level == level)
        .map(|count| count.count)
        .unwrap_or_default()
}
