//! Markdown report renderer — produces a human-readable text summary with
//! heading levels, bullet lists, and metadata.

use super::{Report, ReportResult, ReportWriter};
use crate::model::LogLevel;

/// Report writer that outputs Markdown.
#[derive(Debug, Default, Clone, Copy)]
pub struct MarkdownReportWriter;

impl ReportWriter for MarkdownReportWriter {
    fn write(&self, report: &Report) -> ReportResult<String> {
        let mut output = format!(
            "# {}\n\nTotal entries: {}\n\n## Level counts\n",
            report.title, report.summary.total_count
        );

        if let Some(metadata) = &report.metadata {
            output.push_str(&format!(
                "\nSource: {}\nGenerated at: {}\n",
                metadata.source, metadata.generated_at.value
            ));
        }

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
