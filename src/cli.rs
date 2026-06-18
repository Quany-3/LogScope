pub const MODULE_NAME: &str = "cli";

use crate::analyzer::{AnalysisResult, AnalysisService, BasicAnalyzer};
use crate::config::{LogScopeConfig, ParserFormat};
use crate::parser::{JsonLineLogParser, LogParser, PlainTextLogParser};
use anyhow::{Context, Result};
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ParserKind {
    Text,
    Json,
}

/// Execute the requested command and return its analysis data.
pub fn execute(cli: &Cli) -> Result<AnalysisResult> {
    match &cli.command {
        Command::Analyze(args) => execute_analyze(args),
    }
}

fn execute_analyze(args: &AnalyzeArgs) -> Result<AnalysisResult> {
    let options = resolve_options(args)?;
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

    Ok(BasicAnalyzer.analyze(&entries))
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let result = execute(&cli)?;
    println!("{}", format_analysis_summary(&result));
    Ok(())
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
fn resolve_options(args: &AnalyzeArgs) -> Result<RuntimeOptions> {
    let config = args
        .config
        .as_ref()
        .map(LogScopeConfig::load_from_file)
        .transpose()?;

    let input = args
        .input
        .clone()
        .or_else(|| config.as_ref().map(|config| PathBuf::from(&config.input)))
        .context("input file is required when no config file provides one")?;
    let parser = args
        .parser
        .or_else(|| config.as_ref().map(|config| config.parser.into()))
        .unwrap_or(ParserKind::Text);

    Ok(RuntimeOptions { input, parser })
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
    use super::{AnalyzeArgs, Cli, Command, ParserKind, execute};
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

        let result = execute(&cli).unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(result.total_count, 1);
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

        let result = execute(&cli).unwrap();

        fs::remove_file(log_path).unwrap();
        fs::remove_file(config_path).unwrap();
        assert_eq!(result.total_count, 1);
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

        let result = execute(&cli).unwrap();

        fs::remove_file(json_path).unwrap();
        fs::remove_file(config_path).unwrap();
        assert_eq!(result.total_count, 1);
    }

    fn analyze_args(cli: &Cli) -> &AnalyzeArgs {
        let Command::Analyze(args) = &cli.command;
        args
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
