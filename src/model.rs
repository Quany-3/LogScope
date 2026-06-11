use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use thiserror::Error;

pub const MODULE_NAME: &str = "model";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        }
    }

    pub const fn severity(self) -> u8 {
        match self {
            Self::Trace => 0,
            Self::Debug => 1,
            Self::Info => 2,
            Self::Warn => 3,
            Self::Error => 4,
            Self::Fatal => 5,
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LogLevel {
    type Err = LogLevelParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_uppercase().as_str() {
            "TRACE" => Ok(Self::Trace),
            "DEBUG" => Ok(Self::Debug),
            "INFO" => Ok(Self::Info),
            "WARN" | "WARNING" => Ok(Self::Warn),
            "ERROR" => Ok(Self::Error),
            "FATAL" | "CRITICAL" => Ok(Self::Fatal),
            _ => Err(LogLevelParseError {
                token: value.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown log level `{token}`")]
pub struct LogLevelParseError {
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
    pub raw: String,
}

impl LogEntry {
    pub fn new(
        timestamp: DateTime<Utc>,
        level: LogLevel,
        message: impl Into<String>,
        raw: impl Into<String>,
    ) -> Self {
        Self {
            timestamp,
            level,
            message: message.into(),
            raw: raw.into(),
        }
    }
}
