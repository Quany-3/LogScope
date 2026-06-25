//! LogScope — a terminal log analysis toolkit.
//!
//! Parses plain-text and JSON log files into a unified [`LogEntry`] model, then
//! provides filtering, keyword search, pattern detection, and report generation
//! through both a CLI and an interactive TUI.

pub mod analyzer;
pub mod cli;
pub mod config;
pub mod filter;
pub mod model;
pub mod parser;
pub mod report;
pub mod tui;
pub mod utils;
