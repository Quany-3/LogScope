pub const MODULE_NAME: &str = "cli";

use crate::config::{LogScopeConfig, ParserFormat};
use crate::parser::{JsonLineLogParser, LogParser, PlainTextLogParser};
use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
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

/// Parse the selected input file and return the number of accepted log entries.
pub fn execute(cli: &Cli) -> Result<usize> {
    let options = resolve_options(cli)?;
    let content = fs::read_to_string(&options.input)
        .with_context(|| format!("failed to read input file {}", options.input.display()))?;

    let parser = parser_for(options.parser);
    let mut parsed_count = 0;
    for (index, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        parser
            .parse_line(line)
            .with_context(|| format!("failed to parse line {}", index + 1))?;
        parsed_count += 1;
    }

    Ok(parsed_count)
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let parsed_count = execute(&cli)?;
    println!("parsed {parsed_count} log entries");
    Ok(())
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
fn resolve_options(cli: &Cli) -> Result<RuntimeOptions> {
    let config = cli
        .config
        .as_ref()
        .map(LogScopeConfig::load_from_file)
        .transpose()?;

    let input = cli
        .input
        .clone()
        .or_else(|| config.as_ref().map(|config| PathBuf::from(&config.input)))
        .context("input file is required when no config file provides one")?;
    let parser = cli
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
    use super::{Cli, ParserKind, execute};
    use clap::Parser;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_input_file_argument() {
        let cli = Cli::try_parse_from(["logscope", "logs/app.log"]).unwrap();

        assert_eq!(cli.input, Some(PathBuf::from("logs/app.log")));
    }

    #[test]
    fn parses_parser_type_option() {
        let cli = Cli::try_parse_from(["logscope", "--parser", "text", "logs/app.log"]).unwrap();

        assert_eq!(cli.parser, Some(ParserKind::Text));
    }

    #[test]
    fn parses_json_parser_type_option() {
        let cli = Cli::try_parse_from(["logscope", "--parser", "json", "logs/app.json"]).unwrap();

        assert_eq!(cli.parser, Some(ParserKind::Json));
    }

    #[test]
    fn parses_config_file_option_without_input_argument() {
        let cli = Cli::try_parse_from(["logscope", "--config", "logscope.toml"]).unwrap();

        assert_eq!(cli.config, Some(PathBuf::from("logscope.toml")));
        assert_eq!(cli.input, None);
    }

    #[test]
    fn connects_cli_input_with_plain_text_parser() {
        let path = write_temp_log("2026-06-12T10:00:00Z INFO api request completed\n");
        let cli = Cli {
            input: Some(path.clone()),
            parser: Some(ParserKind::Text),
            config: None,
        };

        let parsed_count = execute(&cli).unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(parsed_count, 1);
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
        let cli = Cli {
            input: None,
            parser: None,
            config: Some(config_path.clone()),
        };

        let parsed_count = execute(&cli).unwrap();

        fs::remove_file(log_path).unwrap();
        fs::remove_file(config_path).unwrap();
        assert_eq!(parsed_count, 1);
    }

    #[test]
    fn cli_values_override_config_defaults() {
        let json_path = write_temp_file(
            "json.log",
            "{\"timestamp\":\"2026-06-12T10:00:00Z\",\"level\":\"INFO\",\"source\":\"api\",\"message\":\"started\"}\n",
        );
        let config_path =
            write_temp_file("toml", "input = \"samples/plain.log\"\nparser = \"text\"\n");
        let cli = Cli {
            input: Some(json_path.clone()),
            parser: Some(ParserKind::Json),
            config: Some(config_path.clone()),
        };

        let parsed_count = execute(&cli).unwrap();

        fs::remove_file(json_path).unwrap();
        fs::remove_file(config_path).unwrap();
        assert_eq!(parsed_count, 1);
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
