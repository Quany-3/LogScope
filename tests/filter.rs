use log_scope::{
    filter::{FilterChain, KeywordFilter, LevelFilter, TimeRangeFilter, filter_entries},
    model::{LogEntry, LogLevel},
    parser::parse_line,
};

fn entry(line: &str) -> LogEntry {
    match parse_line(line) {
        Ok(entry) => entry,
        Err(error) => panic!("expected `{line}` to parse, got {error}"),
    }
}

fn sample_entries() -> Vec<LogEntry> {
    vec![
        entry("[2026-06-11 11:59:00] INFO service started"),
        entry("[2026-06-11 12:00:00] WARN cache miss for user 42"),
        entry("[2026-06-11 12:01:00] ERROR timeout while calling database"),
        entry("[2026-06-11 12:02:00] FATAL database unavailable"),
    ]
}

#[test]
fn filters_by_exact_levels() {
    let entries = sample_entries();
    let filter = LevelFilter::new([LogLevel::Error, LogLevel::Fatal]);
    let matched = filter_entries(&entries, &filter);

    assert_eq!(matched.len(), 2);
    assert_eq!(matched[0].level, LogLevel::Error);
    assert_eq!(matched[1].level, LogLevel::Fatal);
}

#[test]
fn filters_by_keyword_case_insensitively_by_default() {
    let entries = sample_entries();
    let filter = KeywordFilter::new("DATABASE");
    let matched = filter_entries(&entries, &filter);

    assert_eq!(matched.len(), 2);
    assert!(matched.iter().all(|entry| entry.raw.contains("database")));
}

#[test]
fn supports_case_sensitive_keyword_filtering() {
    let lower = entry("[2026-06-11 12:00:00] INFO cache warm");
    let upper = entry("[2026-06-11 12:00:01] INFO Cache warm");
    let entries = vec![lower, upper];
    let filter = KeywordFilter::case_sensitive("Cache");
    let matched = filter_entries(&entries, &filter);

    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].message, "Cache warm");
}

#[test]
fn filters_by_inclusive_time_range() {
    let entries = sample_entries();
    let start = entries[1].timestamp;
    let end = entries[2].timestamp;
    let filter = TimeRangeFilter::new(Some(start), Some(end));
    let matched = filter_entries(&entries, &filter);

    assert_eq!(matched.len(), 2);
    assert_eq!(matched[0].message, "cache miss for user 42");
    assert_eq!(matched[1].message, "timeout while calling database");
}

#[test]
fn combines_filters_with_all_semantics() {
    let entries = sample_entries();
    let chain = FilterChain::new()
        .with(LevelFilter::single(LogLevel::Error))
        .with(KeywordFilter::new("timeout"))
        .with(TimeRangeFilter::after(entries[1].timestamp));

    let matched = filter_entries(&entries, &chain);

    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].level, LogLevel::Error);
    assert_eq!(matched[0].message, "timeout while calling database");
}

#[test]
fn empty_chain_matches_everything() {
    let entries = sample_entries();
    let chain = FilterChain::new();

    assert!(chain.is_empty());
    assert_eq!(filter_entries(&entries, &chain).len(), entries.len());
}
