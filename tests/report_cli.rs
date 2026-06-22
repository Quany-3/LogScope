use clap::Parser;
use log_scope::cli::{Cli, CommandOutput, execute, format_command_output};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn exports_markdown_report_from_cli() {
    let output = temp_path("md");
    let cli = Cli::try_parse_from([
        "logscope",
        "report",
        "--format",
        "markdown",
        "--output",
        output.to_str().unwrap(),
        "samples/plain.log",
    ])
    .unwrap();

    let result = execute(&cli).unwrap();

    let CommandOutput::Report(path) = &result else {
        panic!("expected report output");
    };
    let content = fs::read_to_string(path).unwrap();
    assert!(content.starts_with("# LogScope Analysis Report"));
    assert!(content.contains("## Error Patterns"));
    assert!(format_command_output(&result).contains("Report written to:"));
    fs::remove_file(path).unwrap();
}

#[test]
fn exports_json_report_with_metadata() {
    let output = temp_path("json");
    let cli = Cli::try_parse_from([
        "logscope",
        "report",
        "--parser",
        "json",
        "--format",
        "json",
        "--output",
        output.to_str().unwrap(),
        "samples/json.log",
    ])
    .unwrap();

    execute(&cli).unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
    assert_eq!(json["metadata"]["source"], "samples/json.log");
    assert_eq!(json["metadata"]["entry_count"], 3);
    fs::remove_file(output).unwrap();
}

#[test]
fn exports_report_using_configured_output() {
    let output = temp_path("json");
    let config = temp_path("toml");
    let escaped_output = output.display().to_string().replace('\\', "\\\\");
    fs::write(
        &config,
        format!(
            "input = \"samples/plain.log\"\nparser = \"text\"\n\n[report]\npath = \"{escaped_output}\"\nformat = \"json\"\n"
        ),
    )
    .unwrap();
    let cli =
        Cli::try_parse_from(["logscope", "report", "--config", config.to_str().unwrap()]).unwrap();

    execute(&cli).unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
    assert_eq!(json["summary"]["total_count"], 3);
    fs::remove_file(output).unwrap();
    fs::remove_file(config).unwrap();
}

fn temp_path(extension: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("logscope-report-{suffix}.{extension}"))
}
