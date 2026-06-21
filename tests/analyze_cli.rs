use clap::Parser;
use log_scope::cli::{Cli, Command, CommandOutput, ParserKind, execute, format_analysis_summary};
use log_scope::model::LogLevel;

#[test]
fn parses_analyze_subcommand() {
    let cli = Cli::try_parse_from([
        "logscope",
        "analyze",
        "--parser",
        "text",
        "samples/plain.log",
    ])
    .unwrap();

    let Command::Analyze(args) = cli.command else {
        panic!("expected analyze command");
    };
    assert_eq!(args.input.unwrap().to_string_lossy(), "samples/plain.log");
    assert_eq!(args.parser, Some(ParserKind::Text));
}

#[test]
fn connects_parser_and_analyzer_workflow() {
    let cli = Cli::try_parse_from(["logscope", "analyze", "samples/plain.log"]).unwrap();

    let CommandOutput::Analysis(result) = execute(&cli).unwrap() else {
        panic!("expected analysis output");
    };

    assert_eq!(result.basic.total_count, 3);
    assert_eq!(result.basic.level_counts[&LogLevel::Info], 1);
    assert_eq!(result.basic.level_counts[&LogLevel::Warn], 1);
    assert_eq!(result.basic.level_counts[&LogLevel::Error], 1);
    assert_eq!(result.basic.source_counts["api"], 2);
}

#[test]
fn formats_basic_analysis_summary() {
    let cli = Cli::try_parse_from(["logscope", "analyze", "samples/plain.log"]).unwrap();
    let CommandOutput::Analysis(result) = execute(&cli).unwrap() else {
        panic!("expected analysis output");
    };

    let summary = format_analysis_summary(&result.basic);

    assert!(summary.contains("Total entries: 3"));
    assert!(summary.contains("INFO: 1"));
    assert!(summary.contains("api: 2"));
}
