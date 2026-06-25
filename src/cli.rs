//! Command-line interface built on [`clap`].
//!
//! Defines the `logscope` binary's subcommands (`analyze`, `search`, `report`,
//! `tui`), their arguments, and the glue code that wires parsing, analysis,
//! and report generation together.

/// Module identifier used for diagnostics and internal logging.
pub const MODULE_NAME: &str = "cli";

use crate::analyzer::{AnalysisResult, BasicAnalyzer, SourceRanking};
use crate::config::{LogScopeConfig, ParserFormat};
use crate::filter::filter_entries;
use crate::model::{
    ErrorPattern, FilterCondition, LogEntry, LogLevel, LogTimestamp, ReportExportFormat,
    ReportMetadata,
};
use crate::parser::{JsonLineLogParser, PlainTextLogParser, parse_file_auto_with, parse_file_with};
use crate::report::{
    HtmlReportWriter, JsonReportWriter, MarkdownReportWriter, Report, ReportSectionBuilder,
    ReportWriter, build_diagnostic_section, build_insight_section,
};
use crate::utils::write_file_safely;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Command line options for the LogScope binary.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "logscope",
    version,
    about = "Analyze log files from the terminal"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Parse a log file and print basic statistics.
    Analyze(AnalyzeArgs),
    /// Search and filter parsed log entries.
    Search(SearchArgs),
    /// Analyze logs and export a Markdown or JSON report.
    Report(ReportArgs),
    /// Open the interactive terminal interface.
    Tui(TuiArgs),
}

/// Arguments for the `analyze` subcommand.
#[derive(Debug, Clone, Args)]
pub struct AnalyzeArgs {
    /// Input log files to parse.
    #[arg(required_unless_present = "config")]
    pub input: Vec<PathBuf>,
    /// Parser implementation used for the input file.
    #[arg(long = "parser", value_enum)]
    pub parser: Option<ParserKind>,
    /// Optional TOML file providing input and parser defaults.
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Number of source and error-pattern rankings to display.
    #[arg(long, default_value_t = 5)]
    pub top: usize,
    /// Requests at or above this duration are considered slow.
    #[arg(long, default_value_t = 1_000)]
    pub slow_threshold_ms: u64,
}

/// Arguments for the `search` subcommand.
#[derive(Debug, Clone, Args)]
pub struct SearchArgs {
    /// Input log files to search.
    #[arg(required_unless_present = "config")]
    pub input: Vec<PathBuf>,
    #[arg(long = "parser", value_enum)]
    pub parser: Option<ParserKind>,
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long)]
    pub keyword: Option<String>,
    #[arg(long, value_parser = parse_log_level)]
    pub level: Option<LogLevel>,
    #[arg(long)]
    pub source: Option<String>,
    #[arg(long, value_parser = parse_utc_timestamp)]
    pub start: Option<DateTime<Utc>>,
    #[arg(long, value_parser = parse_utc_timestamp)]
    pub end: Option<DateTime<Utc>>,
}

/// Arguments for the `report` subcommand.
#[derive(Debug, Clone, Args)]
pub struct ReportArgs {
    #[arg(required_unless_present = "config")]
    pub input: Vec<PathBuf>,
    #[arg(long = "parser", value_enum)]
    pub parser: Option<ParserKind>,
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long, value_parser = parse_report_format)]
    pub format: Option<ReportExportFormat>,
    #[arg(long, default_value = "LogScope Analysis Report")]
    pub title: String,
    #[arg(long, default_value_t = 5)]
    pub top: usize,
    #[arg(long, default_value_t = 1_000)]
    pub slow_threshold_ms: u64,
}

#[derive(Debug, Clone, Args)]
pub struct TuiArgs {
    /// Optional log files to load when the TUI starts.
    pub input: Vec<PathBuf>,
    #[arg(long = "parser", value_enum)]
    pub parser: Option<ParserKind>,
    #[arg(long)]
    pub config: Option<PathBuf>,
}

/// Parser type selectable from the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ParserKind {
    Auto,
    Text,
    Json,
}

/// Structured output of the `analyze` subcommand (basic stats + rankings + slow requests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedAnalysisOutput {
    pub basic: AnalysisResult,
    pub top_sources: Vec<SourceRanking>,
    pub error_patterns: Vec<ErrorPattern>,
    pub slow_requests: Vec<LogEntry>,
}

/// Tagged union for the different kinds of output a subcommand can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutput {
    Analysis(AdvancedAnalysisOutput),
    Search(Vec<LogEntry>),
    Report(PathBuf),
    Tui,
}

/// Execute the requested command and return structured output.
pub fn execute(cli: &Cli) -> Result<CommandOutput> {
    match &cli.command {
        Command::Analyze(args) => execute_analyze(args).map(CommandOutput::Analysis),
        Command::Search(args) => execute_search(args).map(CommandOutput::Search),
        Command::Report(args) => execute_report(args).map(CommandOutput::Report),
        Command::Tui(args) => execute_tui(args).map(|()| CommandOutput::Tui),
    }
}

fn execute_tui(args: &TuiArgs) -> Result<()> {
    if args.input.is_empty() && args.config.is_none() {
        return crate::tui::run();
    }

    let options = resolve_options(&args.input, args.parser, &args.config)?;
    let entries = load_entries(&options)?;

    crate::tui::run_with_entries(options.source_label(), entries)
}

fn execute_analyze(args: &AnalyzeArgs) -> Result<AdvancedAnalysisOutput> {
    let options = resolve_options(&args.input, args.parser, &args.config)?;
    let entries = load_entries(&options)?;
    let summary = BasicAnalyzer.build_summary(&entries, args.top, args.slow_threshold_ms);

    Ok(AdvancedAnalysisOutput {
        basic: summary.basic,
        top_sources: summary.top_sources,
        error_patterns: summary.error_patterns,
        slow_requests: summary.slow_requests.into_iter().cloned().collect(),
    })
}

fn execute_search(args: &SearchArgs) -> Result<Vec<LogEntry>> {
    let options = resolve_options(&args.input, args.parser, &args.config)?;
    let entries = load_entries(&options)?;
    let condition = FilterCondition {
        keyword: args.keyword.clone(),
        level: args.level,
        source: args.source.clone(),
        start_time: args.start.map(LogTimestamp::new),
        end_time: args.end.map(LogTimestamp::new),
    };
    let result = filter_entries(&entries, &condition);
    Ok(result.entries.into_iter().cloned().collect())
}

fn execute_report(args: &ReportArgs) -> Result<PathBuf> {
    let options = resolve_options(&args.input, args.parser, &args.config)?;
    let entries = load_entries(&options)?;
    let summary = BasicAnalyzer.build_summary(&entries, args.top, args.slow_threshold_ms);
    let insights = BasicAnalyzer.build_insights(&entries, 60, args.slow_threshold_ms, args.top);
    let config = args
        .config
        .as_ref()
        .map(LogScopeConfig::load_from_file)
        .transpose()?;
    let configured_report = config.as_ref().and_then(|config| config.report.as_ref());
    let format = args
        .format
        .or_else(|| configured_report.map(|report| report.format))
        .unwrap_or(ReportExportFormat::Markdown);
    let output = args
        .output
        .clone()
        .or_else(|| configured_report.map(|report| PathBuf::from(&report.path)))
        .unwrap_or_else(|| PathBuf::from(format!("logscope-report.{}", format.extension())));

    let mut source_section = ReportSectionBuilder::new("Top Sources");
    for ranking in &summary.top_sources {
        source_section = source_section.line(format!("{}: {}", ranking.source, ranking.count));
    }
    let mut pattern_section = ReportSectionBuilder::new("Error Patterns");
    for pattern in &summary.error_patterns {
        pattern_section =
            pattern_section.line(format!("{}: {}", pattern.signature, pattern.occurrences));
    }
    let mut slow_section = ReportSectionBuilder::new("Slow Requests");
    for entry in &summary.slow_requests {
        slow_section = slow_section.line(&entry.raw);
    }

    let report = Report {
        title: args.title.clone(),
        metadata: Some(ReportMetadata {
            generated_at: LogTimestamp::new(Utc::now()),
            source: options.source_label(),
            entry_count: entries.len(),
        }),
        summary: summary.basic,
        sections: vec![
            build_diagnostic_section(&insights),
            build_insight_section(&insights),
            source_section.build(),
            pattern_section.build(),
            slow_section.build(),
        ],
    };
    let content = match format {
        ReportExportFormat::Markdown => MarkdownReportWriter.write(&report)?,
        ReportExportFormat::Json => JsonReportWriter.write(&report)?,
        ReportExportFormat::Html => HtmlReportWriter.write(&report)?,
    };
    write_file_safely(&output, &content)?;

    Ok(output)
}

fn load_entries(options: &RuntimeOptions) -> Result<Vec<LogEntry>> {
    let mut entries = Vec::new();
    for input in &options.inputs {
        // Record the physical source file on each entry for later reporting.
        parse_input_file_with(input, options.parser, |mut entry| {
            entry
                .fields
                .insert("origin_file".to_string(), input.display().to_string());
            entries.push(entry);
        })
        .with_context(|| format!("failed to parse input file {}", input.display()))?;
    }
    entries.sort_by_key(|entry| entry.timestamp.value);

    Ok(entries)
}

/// Parse the CLI, execute the requested command, and print the result.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let output = execute(&cli)?;
    println!("{}", format_command_output(&output));
    Ok(())
}

/// Render [`CommandOutput`] as human-readable text for terminal output.
pub fn format_command_output(output: &CommandOutput) -> String {
    match output {
        CommandOutput::Analysis(result) => format_advanced_analysis_summary(result),
        CommandOutput::Search(entries) => {
            let mut display = format!("Matched entries: {}", entries.len());
            for entry in entries {
                display.push('\n');
                display.push_str(&entry.raw);
            }
            display
        }
        CommandOutput::Report(path) => format!("Report written to: {}", path.display()),
        CommandOutput::Tui => "TUI session ended.".to_string(),
    }
}

/// Render the advanced analysis result (basic stats, top sources, error patterns, slow requests).
pub fn format_advanced_analysis_summary(result: &AdvancedAnalysisOutput) -> String {
    let mut display = format_analysis_summary(&result.basic);

    display.push_str("\nTop sources:");
    for ranking in &result.top_sources {
        display.push_str(&format!("\n  {}: {}", ranking.source, ranking.count));
    }

    display.push_str("\nTop error patterns:");
    for pattern in &result.error_patterns {
        display.push_str(&format!(
            "\n  {}: {}",
            pattern.signature, pattern.occurrences
        ));
    }

    display.push_str(&format!("\nSlow requests: {}", result.slow_requests.len()));
    for entry in &result.slow_requests {
        display.push_str("\n  ");
        display.push_str(&entry.raw);
    }

    display
}

/// Build deterministic text output for terminals and integration tests.
pub fn format_analysis_summary(result: &AnalysisResult) -> String {
    let mut summary = format!("Total entries: {}\nLevels:\n", result.total_count);
    for level in [
        crate::model::LogLevel::Trace,
        crate::model::LogLevel::Debug,
        crate::model::LogLevel::Info,
        crate::model::LogLevel::Warn,
        crate::model::LogLevel::Error,
        crate::model::LogLevel::Fatal,
    ] {
        if let Some(count) = result.level_counts.get(&level) {
            summary.push_str(&format!("  {}: {count}\n", level.as_str()));
        }
    }

    summary.push_str("Sources:\n");
    let mut sources = result.source_counts.iter().collect::<Vec<_>>();
    sources.sort_by_key(|(source, _)| *source);
    for (source, count) in sources {
        summary.push_str(&format!("  {source}: {count}\n"));
    }

    summary.trim_end().to_string()
}

/// Dispatch to the correct parser implementation based on the user's selection.
fn parse_input_file_with(
    input: &PathBuf,
    parser: ParserKind,
    visitor: impl FnMut(LogEntry),
) -> Result<()> {
    match parser {
        ParserKind::Auto => Ok(parse_file_auto_with(input, visitor)?),
        ParserKind::Text => Ok(parse_file_with(input, &PlainTextLogParser, visitor)?),
        ParserKind::Json => Ok(parse_file_with(input, &JsonLineLogParser, visitor)?),
    }
}

/// Resolved runtime settings after merging CLI arguments with an optional config file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeOptions {
    inputs: Vec<PathBuf>,
    parser: ParserKind,
}

impl RuntimeOptions {
    /// Human-readable label for the configured input source(s).
    fn source_label(&self) -> String {
        self.inputs
            .iter()
            .map(|input| input.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Resolve effective options with explicit CLI values taking precedence.
fn resolve_options(
    input: &[PathBuf],
    parser: Option<ParserKind>,
    config_path: &Option<PathBuf>,
) -> Result<RuntimeOptions> {
    let config = config_path
        .as_ref()
        .map(LogScopeConfig::load_from_file)
        .transpose()?;

    let inputs = if input.is_empty() {
        // A config file can fully define the runtime when the CLI omits paths.
        config
            .as_ref()
            .map(|config| vec![PathBuf::from(&config.input)])
            .context("input file is required when no config file provides one")?
    } else {
        input.to_vec()
    };
    let parser = parser
        .or_else(|| config.as_ref().map(|config| config.parser.into()))
        .unwrap_or(ParserKind::Auto);

    Ok(RuntimeOptions { inputs, parser })
}

fn parse_log_level(value: &str) -> std::result::Result<LogLevel, String> {
    LogLevel::from_label(value).ok_or_else(|| format!("unsupported log level: {value}"))
}

fn parse_utc_timestamp(value: &str) -> std::result::Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| format!("invalid RFC3339 timestamp: {value}"))
}

fn parse_report_format(value: &str) -> std::result::Result<ReportExportFormat, String> {
    match value.to_ascii_lowercase().as_str() {
        "markdown" | "md" => Ok(ReportExportFormat::Markdown),
        "json" => Ok(ReportExportFormat::Json),
        "html" => Ok(ReportExportFormat::Html),
        _ => Err(format!("unsupported report format: {value}")),
    }
}

impl From<ParserFormat> for ParserKind {
    fn from(value: ParserFormat) -> Self {
        match value {
            ParserFormat::Text => Self::Text,
            ParserFormat::Json => Self::Json,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AnalysisResult, AnalyzeArgs, Cli, Command, CommandOutput, ParserKind, execute};
    use crate::model::ReportExportFormat;
    use clap::Parser;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_input_file_argument() {
        let cli = Cli::try_parse_from(["logscope", "analyze", "logs/app.log"]).unwrap();

        assert_eq!(
            analyze_args(&cli).input,
            vec![PathBuf::from("logs/app.log")]
        );
    }

    #[test]
    fn parses_multiple_input_files_for_analysis() {
        let cli = Cli::try_parse_from([
            "logscope",
            "analyze",
            "--parser",
            "text",
            "logs/api.log",
            "logs/worker.log",
        ])
        .unwrap();

        assert_eq!(
            analyze_args(&cli).input,
            vec![
                PathBuf::from("logs/api.log"),
                PathBuf::from("logs/worker.log")
            ]
        );
    }

    #[test]
    fn parses_tui_subcommand() {
        let cli = Cli::try_parse_from(["logscope", "tui"]).unwrap();

        assert!(matches!(cli.command, Command::Tui(_)));
    }

    #[test]
    fn parses_tui_input_and_parser_options() {
        let cli =
            Cli::try_parse_from(["logscope", "tui", "--parser", "json", "logs/app.json"]).unwrap();
        let Command::Tui(args) = &cli.command else {
            panic!("expected tui command");
        };

        assert_eq!(args.input, vec![PathBuf::from("logs/app.json")]);
        assert_eq!(args.parser, Some(ParserKind::Json));
    }

    #[test]
    fn parses_tui_config_option_without_input() {
        let cli = Cli::try_parse_from(["logscope", "tui", "--config", "logscope.toml"]).unwrap();
        let Command::Tui(args) = &cli.command else {
            panic!("expected tui command");
        };

        assert!(args.input.is_empty());
        assert_eq!(args.config, Some(PathBuf::from("logscope.toml")));
    }

    #[test]
    fn parses_parser_type_option() {
        let cli = Cli::try_parse_from(["logscope", "analyze", "--parser", "text", "logs/app.log"])
            .unwrap();

        assert_eq!(analyze_args(&cli).parser, Some(ParserKind::Text));
    }

    #[test]
    fn parses_json_parser_type_option() {
        let cli = Cli::try_parse_from(["logscope", "analyze", "--parser", "json", "logs/app.json"])
            .unwrap();

        assert_eq!(analyze_args(&cli).parser, Some(ParserKind::Json));
    }

    #[test]
    fn parses_html_report_format_option() {
        let cli = Cli::try_parse_from([
            "logscope",
            "report",
            "--format",
            "html",
            "--output",
            "report.html",
            "logs/app.log",
        ])
        .unwrap();
        let Command::Report(args) = &cli.command else {
            panic!("expected report command");
        };

        assert_eq!(args.format, Some(ReportExportFormat::Html));
    }

    #[test]
    fn parses_auto_parser_type_option() {
        let cli = Cli::try_parse_from(["logscope", "analyze", "--parser", "auto", "logs/app.log"])
            .unwrap();

        assert_eq!(analyze_args(&cli).parser, Some(ParserKind::Auto));
    }

    #[test]
    fn parses_config_file_option_without_input_argument() {
        let cli =
            Cli::try_parse_from(["logscope", "analyze", "--config", "logscope.toml"]).unwrap();

        assert_eq!(
            analyze_args(&cli).config,
            Some(PathBuf::from("logscope.toml"))
        );
        assert!(analyze_args(&cli).input.is_empty());
    }

    #[test]
    fn connects_cli_input_with_plain_text_parser() {
        let path = write_temp_log("2026-06-12T10:00:00Z INFO api request completed\n");
        let cli = analyze_cli(vec![path.clone()], Some(ParserKind::Text), None);

        let output = execute(&cli).unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(analysis_result(&output).total_count, 1);
    }

    #[test]
    fn combines_multiple_input_files_for_analysis() {
        let first = write_temp_log("2026-06-12T10:00:00Z INFO api request completed\n");
        let second = write_temp_log("2026-06-12T10:01:00Z ERROR worker job failed\n");
        let cli = analyze_cli(
            vec![first.clone(), second.clone()],
            Some(ParserKind::Text),
            None,
        );

        let output = execute(&cli).unwrap();

        fs::remove_file(first).unwrap();
        fs::remove_file(second).unwrap();
        assert_eq!(analysis_result(&output).total_count, 2);
    }

    #[test]
    fn defaults_to_auto_parser_for_json_content_in_log_file() {
        let path = write_temp_file(
            "log",
            "{\"timestamp\":\"2026-06-12T10:00:00Z\",\"level\":\"INFO\",\"source\":\"api\",\"message\":\"started\"}\n",
        );
        let cli = analyze_cli(vec![path.clone()], None, None);

        let output = execute(&cli).unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(analysis_result(&output).total_count, 1);
    }

    #[test]
    fn loads_json_parser_and_input_from_config() {
        let log_path = write_temp_file(
            "json.log",
            "{\"timestamp\":\"2026-06-12T10:00:00Z\",\"level\":\"INFO\",\"source\":\"api\",\"message\":\"started\"}\n",
        );
        let escaped_path = log_path.display().to_string().replace('\\', "\\\\");
        let config_path = write_temp_file(
            "toml",
            &format!("input = \"{escaped_path}\"\nparser = \"json\"\n"),
        );
        let cli = analyze_cli(Vec::new(), None, Some(config_path.clone()));

        let output = execute(&cli).unwrap();

        fs::remove_file(log_path).unwrap();
        fs::remove_file(config_path).unwrap();
        assert_eq!(analysis_result(&output).total_count, 1);
    }

    #[test]
    fn cli_values_override_config_defaults() {
        let json_path = write_temp_file(
            "json.log",
            "{\"timestamp\":\"2026-06-12T10:00:00Z\",\"level\":\"INFO\",\"source\":\"api\",\"message\":\"started\"}\n",
        );
        let config_path =
            write_temp_file("toml", "input = \"samples/plain.log\"\nparser = \"text\"\n");
        let cli = analyze_cli(
            vec![json_path.clone()],
            Some(ParserKind::Json),
            Some(config_path.clone()),
        );

        let output = execute(&cli).unwrap();

        fs::remove_file(json_path).unwrap();
        fs::remove_file(config_path).unwrap();
        assert_eq!(analysis_result(&output).total_count, 1);
    }

    fn analyze_args(cli: &Cli) -> &AnalyzeArgs {
        let Command::Analyze(args) = &cli.command else {
            panic!("expected analyze command");
        };
        args
    }

    fn analysis_result(output: &CommandOutput) -> &AnalysisResult {
        let CommandOutput::Analysis(result) = output else {
            panic!("expected analysis output");
        };
        &result.basic
    }

    fn analyze_cli(
        input: Vec<PathBuf>,
        parser: Option<ParserKind>,
        config: Option<PathBuf>,
    ) -> Cli {
        Cli {
            command: Command::Analyze(AnalyzeArgs {
                input,
                parser,
                config,
                top: 5,
                slow_threshold_ms: 1_000,
            }),
        }
    }

    fn write_temp_log(content: &str) -> PathBuf {
        write_temp_file("log", content)
    }

    fn write_temp_file(extension: &str, content: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("logscope-cli-{suffix}.{extension}"));
        fs::write(&path, content).unwrap();
        path
    }
}
