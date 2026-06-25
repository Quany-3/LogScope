//! Report generation pipeline.
//!
//! Defines the format-neutral [`Report`] and [`ReportSection`] data structures,
//! a [`ReportWriter`] trait for concrete renderers (Markdown, JSON, HTML), and
//! helper functions for building sections and truncated previews.

/// Module identifier used for diagnostics and internal logging.
pub const MODULE_NAME: &str = "report";

mod html;
mod json;
mod markdown;
mod sections;

pub use html::HtmlReportWriter;
pub use json::JsonReportWriter;
pub use markdown::MarkdownReportWriter;
pub use sections::{ReportSectionBuilder, build_diagnostic_section, build_insight_section};

use crate::analyzer::AnalysisResult;
use crate::model::ReportMetadata;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// In-memory report payload before it is rendered by a concrete writer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ReportMetadata>,
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
    /// Create a section with the given heading and body text.
    pub fn new(heading: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            heading: heading.into(),
            body: body.into(),
        }
    }
}

/// Result type for report writing operations.
pub type ReportResult<T> = Result<T, ReportError>;

/// Errors that can occur during report generation.
#[derive(Debug, Error)]
pub enum ReportError {
    #[error("failed to serialize JSON report: {0}")]
    Json(#[from] serde_json::Error),
}

/// Format-neutral report writer interface used by CLI and TUI workflows.
pub trait ReportWriter {
    fn write(&self, report: &Report) -> ReportResult<String>;
}

/// Rendered report excerpt that can be displayed without loading a full report view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportPreview {
    pub lines: Vec<String>,
    pub total_lines: usize,
    pub truncated: bool,
}

/// Build a truncated preview of the rendered report for the TUI side panel.
pub fn build_report_preview(
    report: &Report,
    writer: &dyn ReportWriter,
    max_lines: usize,
) -> ReportResult<ReportPreview> {
    let rendered = writer.write(report)?;
    // Store a line slice so the TUI preview can show truncated content without reparsing.
    let lines = rendered.lines().map(str::to_string).collect::<Vec<_>>();
    let total_lines = lines.len();
    let truncated = total_lines > max_lines;

    Ok(ReportPreview {
        lines: lines.into_iter().take(max_lines).collect(),
        total_lines,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        HtmlReportWriter, JsonReportWriter, MarkdownReportWriter, Report, ReportSection,
        ReportSectionBuilder, ReportWriter,
    };
    use crate::analyzer::AnalysisResult;
    use crate::model::LogLevel;

    #[test]
    fn defines_report_data_structures() {
        let report = Report {
            title: "Daily LogScope Report".to_string(),
            metadata: None,
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
        assert!(output.contains("- c.a.d.p.DruidAbstractDataSource: 2"));
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

    #[test]
    fn writes_html_report_with_visual_sections() {
        let report = sample_report();
        let output = render_report(&HtmlReportWriter, &report);

        assert!(output.starts_with("<!doctype html>"));
        assert!(output.contains("<title>Daily LogScope Report</title>"));
        assert!(output.contains("class=\"metric\""));
        assert!(output.contains("Level Distribution"));
        assert!(output.contains("Source Distribution"));
        assert!(output.contains("class=\"donut-card\""));
        assert!(output.contains("conic-gradient("));
        assert!(output.contains("class=\"bar-label\""));
        assert!(output.contains("title=\"c.a.d.p.DruidAbstractDataSource\""));
        assert!(output.contains("Notes"));
    }

    #[test]
    fn builds_limited_report_preview_for_tui() {
        let report = sample_report();
        let preview = super::build_report_preview(&report, &MarkdownReportWriter, 3).unwrap();

        assert_eq!(preview.lines.len(), 3);
        assert_eq!(preview.total_lines, 14);
        assert!(preview.truncated);
        assert_eq!(preview.lines[0], "# Daily LogScope Report");
    }

    #[test]
    fn builds_operational_insight_report_section() {
        let insights = crate::analyzer::OperationalInsights {
            severity_score: 88,
            error_rate_percent: 25,
            slow_rate_percent: 40,
            peak_window: None,
            file_activity: vec![crate::analyzer::FileActivityInsight {
                path: "worker.log".to_string(),
                total_count: 4,
                warning_count: 1,
                error_count: 2,
                fatal_count: 1,
                slow_count: 2,
                error_rate_percent: 50,
                severity_score: 95,
            }],
            correlations: vec![crate::analyzer::CorrelationInsight {
                key: "request_id=req-42".to_string(),
                entry_count: 3,
                error_count: 2,
                sources: vec!["api".to_string(), "db".to_string()],
                sample_messages: vec!["database timeout".to_string()],
            }],
        };

        let section = super::build_insight_section(&insights);

        assert_eq!(section.heading, "Operational Insights");
        assert!(section.body.contains("Severity score: 88/100"));
        assert!(section.body.contains("Error rate: 25%"));
        assert!(
            section
                .body
                .contains("worker.log: severity 95/100, 4 entries, 2 errors, 1 fatal, 2 slow")
        );
        assert!(
            section
                .body
                .contains("request_id=req-42: 3 entries, 2 errors")
        );
    }

    #[test]
    fn builds_diagnostic_finding_report_section() {
        let insights = crate::analyzer::OperationalInsights {
            severity_score: 92,
            error_rate_percent: 40,
            slow_rate_percent: 20,
            peak_window: None,
            file_activity: vec![crate::analyzer::FileActivityInsight {
                path: "worker.log".to_string(),
                total_count: 10,
                warning_count: 1,
                error_count: 6,
                fatal_count: 1,
                slow_count: 3,
                error_rate_percent: 60,
                severity_score: 100,
            }],
            correlations: vec![crate::analyzer::CorrelationInsight {
                key: "job_id=job-77".to_string(),
                entry_count: 4,
                error_count: 3,
                sources: vec!["worker".to_string()],
                sample_messages: vec!["worker crashed".to_string()],
            }],
        };

        let section = super::build_diagnostic_section(&insights);

        assert_eq!(section.heading, "Diagnostic Findings");
        assert!(
            section
                .body
                .contains("Overall health is critical: severity 92/100 with 40% errors.")
        );
        assert!(
            section
                .body
                .contains("Most affected file: worker.log has 6 errors across 10 entries.")
        );
        assert!(
            section
                .body
                .contains("Correlated failures: job_id=job-77 links 4 entries and 3 errors.")
        );
    }

    fn render_report(writer: &dyn ReportWriter, report: &Report) -> String {
        writer.write(report).unwrap()
    }

    fn sample_report() -> Report {
        let mut summary = AnalysisResult::new(3);
        summary.level_counts.insert(LogLevel::Info, 2);
        summary.level_counts.insert(LogLevel::Error, 1);
        summary
            .source_counts
            .insert("c.a.d.p.DruidAbstractDataSource".to_string(), 2);
        summary.source_counts.insert("worker".to_string(), 1);

        Report {
            title: "Daily LogScope Report".to_string(),
            metadata: None,
            summary,
            sections: vec![ReportSection::new("Notes", "Generated from sample logs.")],
        }
    }
}
