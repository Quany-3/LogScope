use crate::analyzer::{KeywordCount, LevelCount, LogStats};

pub const MODULE_NAME: &str = "report";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Text,
    Json,
}

pub fn render_report(stats: &LogStats, format: ReportFormat) -> Result<String, serde_json::Error> {
    match format {
        ReportFormat::Text => Ok(render_text(stats)),
        ReportFormat::Json => serde_json::to_string_pretty(stats),
    }
}

pub fn render_text(stats: &LogStats) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Total lines: {}", stats.total_lines));
    lines.push(format!("Error ratio: {:.2}%", stats.error_ratio * 100.0));

    if let Some(span) = &stats.time_span {
        lines.push(format!(
            "Time span: {} -> {} ({}s)",
            span.start.to_rfc3339(),
            span.end.to_rfc3339(),
            span.duration_seconds
        ));
    } else {
        lines.push("Time span: n/a".to_string());
    }

    lines.push("Level counts:".to_string());
    for LevelCount { level, count } in &stats.level_counts {
        lines.push(format!("  {}: {}", level, count));
    }

    lines.push("Top keywords:".to_string());
    if stats.top_keywords.is_empty() {
        lines.push("  n/a".to_string());
    } else {
        for KeywordCount { keyword, count } in &stats.top_keywords {
            lines.push(format!("  {}: {}", keyword, count));
        }
    }

    lines.push("Error samples:".to_string());
    if stats.error_samples.is_empty() {
        lines.push("  n/a".to_string());
    } else {
        for sample in &stats.error_samples {
            lines.push(format!(
                "  [{}] {} {}",
                sample.level,
                sample.timestamp.to_rfc3339(),
                sample.message
            ));
        }
    }

    lines.join("\n")
}
