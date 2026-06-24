use super::{Report, ReportResult, ReportWriter};

#[derive(Debug, Default, Clone, Copy)]
pub struct JsonReportWriter;

impl ReportWriter for JsonReportWriter {
    fn write(&self, report: &Report) -> ReportResult<String> {
        Ok(serde_json::to_string_pretty(report)?)
    }
}
