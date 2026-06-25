//! TOML-based configuration for LogScope defaults.
//!
//! A config file can supply the input path, parser format, and report output
//! settings so users don't have to repeat them on every CLI invocation.

/// Module identifier used for diagnostics and internal logging.
pub const MODULE_NAME: &str = "config";

use crate::model::ReportExportFormat;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// User-facing configuration loaded from a TOML file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogScopeConfig {
    pub input: String,
    pub parser: ParserFormat,
    #[serde(default)]
    pub report: Option<ReportOutputConfig>,
}

/// Report output settings that can be set via config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportOutputConfig {
    pub path: String,
    pub format: ReportExportFormat,
}

/// Parser format selectable from the config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParserFormat {
    Text,
    Json,
}

impl LogScopeConfig {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse config file {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::{LogScopeConfig, ParserFormat, ReportOutputConfig};
    use crate::model::ReportExportFormat;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn defines_logscope_configuration_structure() {
        let config = LogScopeConfig {
            input: "samples/plain.log".into(),
            parser: ParserFormat::Text,
            report: None,
        };

        assert_eq!(config.input, "samples/plain.log");
        assert_eq!(config.parser, ParserFormat::Text);
    }

    #[test]
    fn loads_config_from_toml_file() {
        let path = temp_config_path();
        fs::write(&path, "input = \"samples/json.log\"\nparser = \"json\"\n").unwrap();

        let config = LogScopeConfig::load_from_file(&path).unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(config.input, "samples/json.log");
        assert_eq!(config.parser, ParserFormat::Json);
    }

    #[test]
    fn loads_report_output_configuration() {
        let path = temp_config_path();
        fs::write(
            &path,
            "input = \"samples/plain.log\"\nparser = \"text\"\n\n[report]\npath = \"reports/summary.md\"\nformat = \"markdown\"\n",
        )
        .unwrap();

        let config = LogScopeConfig::load_from_file(&path).unwrap();

        fs::remove_file(path).unwrap();
        assert_eq!(
            config.report,
            Some(ReportOutputConfig {
                path: "reports/summary.md".to_string(),
                format: ReportExportFormat::Markdown,
            })
        );
    }

    fn temp_config_path() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("logscope-config-{suffix}.toml"))
    }
}
