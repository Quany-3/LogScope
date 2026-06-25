//! JSON report renderer — serializes the [`Report`] struct as pretty-printed JSON.

use super::{Report, ReportResult, ReportWriter};

/// Report writer that outputs pretty-printed JSON.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonReportWriter;

impl ReportWriter for JsonReportWriter {
    fn write(&self, report: &Report) -> ReportResult<String> {
        Ok(serde_json::to_string_pretty(report)?)
    }
}
