pub const MODULE_NAME: &str = "cli";

use crate::analyzer::{AnalysisResult, BasicAnalyzer, SourceRanking};
use crate::config::{LogScopeConfig, ParserFormat};
use crate::model::{
    ErrorPattern, FilterCondition, LogEntry, LogLevel, LogTimestamp, ReportExportFormat,
    ReportMetadata, SearchResult,
};
use crate::parser::{JsonLineLogParser, LogParser, PlainTextLogParser, parse_file};
use crate::report::{
    JsonReportWriter, MarkdownReportWriter, Report, ReportSectionBuilder, ReportWriter,
    build_insight_section,
};
use crate::utils::write_file_safely;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::fs;
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

#[derive(Debug, Clone, Args)]
pub struct AnalyzeArgs {
    /// Input log file to parse.
    #[arg(required_unless_present = "config")]
    pub input: Option<PathBuf>,
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

#[derive(Debug, Clone, Args)]
pub struct SearchArgs {
    /// Input log file to search.
    #[arg(required_unless_present = "config")]
    pub input: Option<PathBuf>,
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

#[derive(Debug, Clone, Args)]
pub struct ReportArgs {
    #[arg(required_unless_present = "config")]
    pub input: Option<PathBuf>,
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
    /// Optional log file to load when the TUI starts.
    pub input: Option<PathBuf>,
    #[arg(long = "parser", value_enum)]
    pub parser: Option<ParserKind>,
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ParserKind {
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedAnalysisOutput {
    pub basic: AnalysisResult,
    pub top_sources: Vec<SourceRanking>,
    pub error_patterns: Vec<ErrorPattern>,
    pub slow_requests: Vec<LogEntry>,
}

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
    if args.input.is_none() && args.config.is_none() {
        return crate::tui::run();
    }

    let options = resolve_options(&args.input, args.parser, &args.config)?;
    let parser = parser_for(options.parser);
    let entries = parse_file(&options.input, parser.as_ref())
        .with_context(|| format!("failed to parse input file {}", options.input.display()))?;

    crate::tui::run_with_entries(options.input.display().to_string(), entries)
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
    let analyzer = BasicAnalyzer;
    let mut matches = entries.iter().collect::<Vec<_>>();

    if let Some(keyword) = condition.keyword.as_deref() {
        retain_allowed(&mut matches, analyzer.search_keyword(&entries, keyword));
    }
    if let Some(level) = condition.level {
        retain_allowed(&mut matches, analyzer.filter_by_level(&entries, level));
    }
    if let Some(source) = condition.source.as_deref() {
        retain_allowed(&mut matches, analyzer.filter_by_source(&entries, source));
    }
    if condition.start_time.is_some() || condition.end_time.is_some() {
        let start = condition
            .start_time
            .map(|timestamp| timestamp.value)
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        let end = condition
            .end_time
            .map(|timestamp| timestamp.value)
            .unwrap_or(DateTime::<Utc>::MAX_UTC);
        retain_allowed(
            &mut matches,
            analyzer.filter_by_time_range(&entries, start, end),
        );
    }

    let result = SearchResult::new(matches);
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
            source: options.input.display().to_string(),
            entry_count: entries.len(),
        }),
        summary: summary.basic,
        sections: vec![
            build_insight_section(&insights),
            source_section.build(),
            pattern_section.build(),
            slow_section.build(),
        ],
    };
    let content = match format {
        ReportExportFormat::Markdown => MarkdownReportWriter.write(&report)?,
        ReportExportFormat::Json => JsonReportWriter.write(&report)?,
    };
    write_file_safely(&output, &content)?;

    Ok(output)
}

fn load_entries(options: &RuntimeOptions) -> Result<Vec<LogEntry>> {
    let content = fs::read_to_string(&options.input)
        .with_context(|| format!("failed to read input file {}", options.input.display()))?;

    let parser = parser_for(options.parser);
    let mut entries = Vec::new();
    for (index, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry = parser
            .parse_line(line)
            .with_context(|| format!("failed to parse line {}", index + 1))?;
        entries.push(entry);
    }

    Ok(entries)
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let output = execute(&cli)?;
    println!("{}", format_command_output(&output));
    Ok(())
}

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

fn parser_for(parser: ParserKind) -> Box<dyn LogParser> {
    match parser {
        ParserKind::Text => Box::new(PlainTextLogParser),
        ParserKind::Json => Box::new(JsonLineLogParser),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeOptions {
    input: PathBuf,
    parser: ParserKind,
}

/// Resolve effective options with explicit CLI values taking precedence.
fn resolve_options(
    input: &Option<PathBuf>,
    parser: Option<ParserKind>,
    config_path: &Option<PathBuf>,
) -> Result<RuntimeOptions> {
    let config = config_path
        .as_ref()
        .map(LogScopeConfig::load_from_file)
        .transpose()?;

    let input = input
        .clone()
        .or_else(|| config.as_ref().map(|config| PathBuf::from(&config.input)))
        .context("input file is required when no config file provides one")?;
    let parser = parser
        .or_else(|| config.as_ref().map(|config| config.parser.into()))
        .unwrap_or(ParserKind::Text);

    Ok(RuntimeOptions { input, parser })
}

fn retain_allowed<'a>(current: &mut Vec<&'a LogEntry>, allowed: Vec<&'a LogEntry>) {
    current.retain(|entry| allowed.contains(entry));
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
    use clap::Parser;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_input_file_argument() {
        let cli = Cli::try_parse_from(["logscope", "analyze", "logs/app.log"]).unwrap();

        assert_eq!(
            analyze_args(&cli).input,
            Some(PathBuf::from("logs/app.log"))
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

        assert_eq!(args.input, Some(PathBuf::from("logs/app.json")));
        assert_eq!(args.parser, Some(ParserKind::Json));
    }

    #[test]
    fn parses_tui_config_option_without_input() {
        let cli = Cli::try_parse_from(["logscope", "tui", "--config", "logscope.toml"]).unwrap();
        let Command::Tui(args) = &cli.command else {
            panic!("expected tui command");
        };

        assert_eq!(args.input, None);
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
    fn parses_config_file_option_without_input_argument() {
        let cli =
            Cli::try_parse_from(["logscope", "analyze", "--config", "logscope.toml"]).unwrap();

        assert_eq!(
            analyze_args(&cli).config,
            Some(PathBuf::from("logscope.toml"))
        );
        assert_eq!(analyze_args(&cli).input, None);
    }

    #[test]
    fn connects_cli_input_with_plain_text_parser() {
        let path = write_temp_log("2026-06-12T10:00:00Z INFO api request completed\n");
        let cli = analyze_cli(Some(path.clone()), Some(ParserKind::Text), None);

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
        let cli = analyze_cli(None, None, Some(config_path.clone()));

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
            Some(json_path.clone()),
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
        input: Option<PathBuf>,
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
