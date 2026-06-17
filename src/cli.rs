pub const MODULE_NAME: &str = "cli";

use crate::parser::{LogParser, PlainTextLogParser};
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
    pub input: PathBuf,
    /// Parser implementation used for the input file.
    #[arg(long = "parser", value_enum, default_value_t = ParserKind::Text)]
    pub parser: ParserKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ParserKind {
    Text,
}

/// Parse the selected input file and return the number of accepted log entries.
pub fn execute(cli: &Cli) -> Result<usize> {
    let content = fs::read_to_string(&cli.input)
        .with_context(|| format!("failed to read input file {}", cli.input.display()))?;

    let parser = parser_for(cli.parser);
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

        assert_eq!(cli.input, PathBuf::from("logs/app.log"));
    }

    #[test]
    fn parses_parser_type_option() {
        let cli = Cli::try_parse_from(["logscope", "--parser", "text", "logs/app.log"]).unwrap();

        assert_eq!(cli.parser, ParserKind::Text);
    }

    #[test]
    fn connects_cli_input_with_plain_text_parser() {
        let path = write_temp_log("2026-06-12T10:00:00Z INFO api request completed\n");
        let cli = Cli {
            input: path.clone(),
            parser: ParserKind::Text,
        };

        let parsed_count = execute(&cli).unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(parsed_count, 1);
    }

    fn write_temp_log(content: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("logscope-cli-{suffix}.log"));
        fs::write(&path, content).unwrap();
        path
    }
}
