use clap::Parser;
use log_scope::cli::{Cli, CommandOutput, execute, format_command_output};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn runs_advanced_analysis_end_to_end() {
    let path = write_temp_log(
        "2026-06-12T10:00:00Z INFO api completed duration_ms=50\n\
         2026-06-12T10:01:00Z ERROR api database_timeout status=500 duration_ms=1500\n\
         2026-06-12T10:02:00Z ERROR worker database_timeout status=503 duration_ms=1100\n",
    );
    let cli = Cli::try_parse_from([
        "logscope",
        "analyze",
        "--top",
        "2",
        "--slow-threshold-ms",
        "1000",
        path.to_str().unwrap(),
    ])
    .unwrap();

    let CommandOutput::Analysis(result) = execute(&cli).unwrap() else {
        panic!("expected analysis output");
    };

    fs::remove_file(path).unwrap();
    assert_eq!(result.basic.total_count, 3);
    assert_eq!(result.top_sources[0].source, "api");
    assert_eq!(result.top_sources[0].count, 2);
    assert_eq!(result.error_patterns[0].signature, "database_timeout");
    assert_eq!(result.error_patterns[0].occurrences, 2);
    assert_eq!(result.slow_requests.len(), 2);
}

#[test]
fn formats_advanced_analysis_sections() {
    let cli = Cli::try_parse_from(["logscope", "analyze", "samples/plain.log"]).unwrap();
    let output = execute(&cli).unwrap();

    let display = format_command_output(&output);

    assert!(display.contains("Top sources:"));
    assert!(display.contains("Top error patterns:"));
    assert!(display.contains("database timeout: 1"));
    assert!(display.contains("Slow requests: 0"));
}

fn write_temp_log(content: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("logscope-advanced-{suffix}.log"));
    fs::write(&path, content).unwrap();
    path
}
