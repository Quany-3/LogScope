//! Report section builders that turn operational insights into prose.

use super::ReportSection;
use crate::analyzer::OperationalInsights;

/// Build the "Operational Insights" section listing severity, rates, peak window,
/// file activity, and correlated activity.
pub fn build_insight_section(insights: &OperationalInsights) -> ReportSection {
    let mut builder = ReportSectionBuilder::new("Operational Insights")
        .line(format!("Severity score: {}/100", insights.severity_score))
        .line(format!("Error rate: {}%", insights.error_rate_percent))
        .line(format!(
            "Slow request rate: {}%",
            insights.slow_rate_percent
        ));

    if let Some(window) = &insights.peak_window {
        // Surface the busiest period first because it usually explains the spike.
        builder = builder.line(format!(
            "Peak window: {} to {} ({} entries, {} errors, {} warnings)",
            window.start, window.end, window.entry_count, window.error_count, window.warning_count
        ));
    }

    if !insights.file_activity.is_empty() {
        builder = builder.line("File activity:");
        for file in &insights.file_activity {
            builder = builder.line(format!(
                "{}: severity {}/100, {} entries, {} errors, {} fatal, {} slow",
                file.path,
                file.severity_score,
                file.total_count,
                file.error_count,
                file.fatal_count,
                file.slow_count
            ));
        }
    }

    if !insights.correlations.is_empty() {
        builder = builder.line("Correlated activity:");
        for group in &insights.correlations {
            builder = builder.line(format!(
                "{}: {} entries, {} errors, sources={}",
                group.key,
                group.entry_count,
                group.error_count,
                group.sources.join(",")
            ));
        }
    }

    builder.build()
}

/// Build the "Diagnostic Findings" section that translates numeric scores into
/// plain-language health statements and surfaces the most affected file, peak
/// window, and correlation.
pub fn build_diagnostic_section(insights: &OperationalInsights) -> ReportSection {
    let mut builder = ReportSectionBuilder::new("Diagnostic Findings");

    // Map the numeric score into a plain-language health statement for reports.
    if insights.severity_score >= 80 {
        builder = builder.line(format!(
            "Overall health is critical: severity {}/100 with {}% errors.",
            insights.severity_score, insights.error_rate_percent
        ));
    } else if insights.severity_score >= 50 {
        builder = builder.line(format!(
            "Overall health is degraded: severity {}/100 with {}% errors.",
            insights.severity_score, insights.error_rate_percent
        ));
    } else {
        builder = builder.line(format!(
            "Overall health is stable: severity {}/100 with {}% errors.",
            insights.severity_score, insights.error_rate_percent
        ));
    }

    if let Some(file) = insights.file_activity.first() {
        builder = builder.line(format!(
            "Most affected file: {} has {} errors across {} entries.",
            file.path, file.error_count, file.total_count
        ));
    }

    if let Some(window) = &insights.peak_window {
        builder = builder.line(format!(
            "Peak incident window: {} to {} contains {} errors and {} warnings.",
            window.start, window.end, window.error_count, window.warning_count
        ));
    }

    if let Some(group) = insights.correlations.first() {
        builder = builder.line(format!(
            "Correlated failures: {} links {} entries and {} errors.",
            group.key, group.entry_count, group.error_count
        ));
    }

    if insights.slow_rate_percent >= 20 {
        builder = builder.line(format!(
            "Latency pressure is elevated: {}% of entries are slow requests.",
            insights.slow_rate_percent
        ));
    }

    builder.build()
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

    /// Append a line to the section body.
    pub fn line(mut self, line: impl Into<String>) -> Self {
        self.lines.push(line.into());
        self
    }

    /// Finalize the builder into a [`ReportSection`], joining accumulated lines with `\n`.
    pub fn build(self) -> ReportSection {
        ReportSection::new(self.heading, self.lines.join("\n"))
    }
}
