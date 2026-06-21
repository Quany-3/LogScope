pub const MODULE_NAME: &str = "report";

use crate::analyzer::AnalysisResult;
use crate::model::LogLevel;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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

pub type ReportResult<T> = Result<T, ReportError>;

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("failed to serialize JSON report: {0}")]
    Json(#[from] serde_json::Error),
}

/// Format-neutral report writer interface used by CLI and TUI workflows.
pub trait ReportWriter {
    fn write(&self, report: &Report) -> ReportResult<String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MarkdownReportWriter;

impl ReportWriter for MarkdownReportWriter {
    fn write(&self, report: &Report) -> ReportResult<String> {
        let mut output = format!(
            "# {}\n\nTotal entries: {}\n\n## Level counts\n",
            report.title, report.summary.total_count
        );

        for level in [
            LogLevel::Trace,
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
            LogLevel::Fatal,
        ] {
            if let Some(count) = report.summary.level_counts.get(&level) {
                output.push_str(&format!("- {}: {count}\n", level.as_str()));
            }
        }

        output.push_str("\n## Source counts\n");
        let mut sources = report.summary.source_counts.iter().collect::<Vec<_>>();
        sources.sort_by_key(|(source, _)| *source);
        for (source, count) in sources {
            output.push_str(&format!("- {source}: {count}\n"));
        }

        for section in &report.sections {
            output.push_str(&format!("\n## {}\n{}\n", section.heading, section.body));
        }

        Ok(output.trim_end().to_string())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct JsonReportWriter;

impl ReportWriter for JsonReportWriter {
    fn write(&self, report: &Report) -> ReportResult<String> {
        Ok(serde_json::to_string_pretty(report)?)
    }
}

/// Small fluent builder for assembling multi-line report sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportSectionBuilder {
    heading: String,
    lines: Vec<String>,
}

impl ReportSectionBuilder {
    pub fn new(heading: impl Into<String>) -> Self {
        Self {
            heading: heading.into(),
            lines: Vec::new(),
        }
    }

    pub fn line(mut self, line: impl Into<String>) -> Self {
        self.lines.push(line.into());
        self
    }

    pub fn build(self) -> ReportSection {
        ReportSection::new(self.heading, self.lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        JsonReportWriter, MarkdownReportWriter, Report, ReportSection, ReportSectionBuilder,
        ReportWriter,
    };
    use crate::analyzer::AnalysisResult;
    use crate::model::LogLevel;

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

    #[test]
    fn builds_report_sections_incrementally() {
        let section = ReportSectionBuilder::new("Error Patterns")
            .line("database timeout: 3")
            .line("connection refused: 2")
            .build();

        assert_eq!(section.heading, "Error Patterns");
        assert_eq!(section.body, "database timeout: 3\nconnection refused: 2");
    }

    #[test]
    fn writes_markdown_report() {
        let report = sample_report();
        let output = render_report(&MarkdownReportWriter, &report);

        assert!(output.starts_with("# Daily LogScope Report"));
        assert!(output.contains("Total entries: 3"));
        assert!(output.contains("- INFO: 2"));
        assert!(output.contains("- api: 2"));
        assert!(output.contains("## Notes\nGenerated from sample logs."));
    }

    #[test]
    fn writes_json_report() {
        let report = sample_report();
        let output = render_report(&JsonReportWriter, &report);
        let json: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(json["title"], "Daily LogScope Report");
        assert_eq!(json["summary"]["total_count"], 3);
        assert_eq!(json["sections"][0]["heading"], "Notes");
    }

    fn render_report(writer: &dyn ReportWriter, report: &Report) -> String {
        writer.write(report).unwrap()
    }

    fn sample_report() -> Report {
        let mut summary = AnalysisResult::new(3);
        summary.level_counts.insert(LogLevel::Info, 2);
        summary.level_counts.insert(LogLevel::Error, 1);
        summary.source_counts.insert("api".to_string(), 2);
        summary.source_counts.insert("worker".to_string(), 1);

        Report {
            title: "Daily LogScope Report".to_string(),
            summary,
            sections: vec![ReportSection::new("Notes", "Generated from sample logs.")],
        }
    }
}
