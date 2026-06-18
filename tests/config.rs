use log_scope::config::{LogScopeConfig, ParserFormat};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn loads_json_parser_config() {
    let path = temp_config_path();
    fs::write(&path, "input = \"samples/json.log\"\nparser = \"json\"\n").unwrap();

    let config = LogScopeConfig::load_from_file(&path).unwrap();

    fs::remove_file(path).unwrap();
    assert_eq!(config.input, "samples/json.log");
    assert_eq!(config.parser, ParserFormat::Json);
}

#[test]
fn loads_text_parser_config() {
    let path = temp_config_path();
    fs::write(&path, "input = \"samples/plain.log\"\nparser = \"text\"\n").unwrap();

    let config = LogScopeConfig::load_from_file(&path).unwrap();

    fs::remove_file(path).unwrap();
    assert_eq!(config.input, "samples/plain.log");
    assert_eq!(config.parser, ParserFormat::Text);
}

#[test]
fn rejects_invalid_toml_config() {
    let path = temp_config_path();
    fs::write(&path, "input = [not valid toml").unwrap();

    let error = LogScopeConfig::load_from_file(&path).unwrap_err();

    fs::remove_file(path).unwrap();
    assert!(error.to_string().contains("failed to parse config file"));
}

// A unique path keeps concurrently running tests from sharing state.
fn temp_config_path() -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("logscope-config-{suffix}.toml"))
}
