pub const MODULE_NAME: &str = "report";

use crate::analyzer::AnalysisResult;
use serde::{Deserialize, Serialize};

/// In-memory report payload before it is rendered as Markdown or JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub title: String,
    pub summary: AnalysisResult,
    pub sections: Vec<ReportSection>,
}

/// A named block of report content, kept format-neutral for later writers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportSection {
    pub heading: String,
    pub body: String,
}

impl ReportSection {
    pub fn new(heading: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            heading: heading.into(),
            body: body.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Report, ReportSection};
    use crate::analyzer::AnalysisResult;

    #[test]
    fn defines_report_data_structures() {
        let report = Report {
            title: "Daily LogScope Report".to_string(),
            summary: AnalysisResult::new(12),
            sections: vec![ReportSection::new("Level Summary", "INFO: 10, ERROR: 2")],
        };

        assert_eq!(report.title, "Daily LogScope Report");
        assert_eq!(report.summary.total_count, 12);
        assert_eq!(report.sections[0].heading, "Level Summary");
    }
}
