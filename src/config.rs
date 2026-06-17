pub const MODULE_NAME: &str = "config";

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// User-facing configuration loaded from a TOML file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogScopeConfig {
    pub input: String,
    pub parser: ParserFormat,
}

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
    use super::{LogScopeConfig, ParserFormat};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn defines_logscope_configuration_structure() {
        let config = LogScopeConfig {
            input: "samples/plain.log".into(),
            parser: ParserFormat::Text,
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

    fn temp_config_path() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("logscope-config-{suffix}.toml"))
    }
}
