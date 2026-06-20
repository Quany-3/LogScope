use clap::Parser;
use log_scope::cli::{Cli, Command, CommandOutput, execute, format_command_output};
use log_scope::model::LogLevel;

#[test]
fn parses_search_subcommand_and_filter_options() {
    let cli = Cli::try_parse_from([
        "logscope",
        "search",
        "--keyword",
        "timeout",
        "--level",
        "error",
        "--source",
        "api",
        "samples/plain.log",
    ])
    .unwrap();

    let Command::Search(args) = cli.command else {
        panic!("expected search command");
    };
    assert_eq!(args.keyword.as_deref(), Some("timeout"));
    assert_eq!(args.level, Some(LogLevel::Error));
    assert_eq!(args.source.as_deref(), Some("api"));
}

#[test]
fn filters_logs_with_search_subcommand() {
    let cli = Cli::try_parse_from([
        "logscope",
        "search",
        "--keyword",
        "timeout",
        "--level",
        "error",
        "--source",
        "api",
        "samples/plain.log",
    ])
    .unwrap();

    let CommandOutput::Search(entries) = execute(&cli).unwrap() else {
        panic!("expected search output");
    };

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "database timeout");
}

#[test]
fn formats_filtered_log_results() {
    let cli = Cli::try_parse_from([
        "logscope",
        "search",
        "--level",
        "error",
        "samples/plain.log",
    ])
    .unwrap();
    let output = execute(&cli).unwrap();

    let display = format_command_output(&output);

    assert!(display.contains("Matched entries: 1"));
    assert!(display.contains("ERROR api database timeout"));
}

#[test]
fn filters_logs_by_cli_time_range() {
    let cli = Cli::try_parse_from([
        "logscope",
        "search",
        "--start",
        "2026-06-12T10:01:00Z",
        "--end",
        "2026-06-12T10:02:00Z",
        "samples/plain.log",
    ])
    .unwrap();

    let CommandOutput::Search(entries) = execute(&cli).unwrap() else {
        panic!("expected search output");
    };

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].level, LogLevel::Warn);
    assert_eq!(entries[1].level, LogLevel::Error);
}
