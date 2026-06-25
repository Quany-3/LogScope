//! HTML report renderer with CSS-only donut and bar charts.
//!
//! The output is a self-contained HTML file with inline styles — no external
//! JavaScript or CSS dependencies. Chart segments use `conic-gradient` so the
//! report remains static and easy to share.

use super::{Report, ReportResult, ReportWriter};
use crate::model::LogLevel;
use std::fmt::Write;

/// Report writer that produces a standalone HTML document.
#[derive(Debug, Default, Clone, Copy)]
pub struct HtmlReportWriter;

impl ReportWriter for HtmlReportWriter {
    fn write(&self, report: &Report) -> ReportResult<String> {
        let mut output = String::from("<!doctype html>\n<html lang=\"en\">\n<head>\n");
        output.push_str("<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
        output.push_str(&format!("<title>{}</title>\n", escape_html(&report.title)));
        output.push_str(HTML_STYLE);
        output.push_str("</head>\n<body>\n<main>\n");
        output.push_str(&format!("<h1>{}</h1>\n", escape_html(&report.title)));

        if let Some(metadata) = &report.metadata {
            output.push_str("<p class=\"meta\">");
            output.push_str(&format!(
                "Source: {} | Entries: {} | Generated: {}",
                escape_html(&metadata.source),
                metadata.entry_count,
                metadata.generated_at.value
            ));
            output.push_str("</p>\n");
        }

        output.push_str("<section class=\"metrics\">\n");
        output.push_str(&metric_card(
            "Total Entries",
            &report.summary.total_count.to_string(),
        ));
        let error_count = report
            .summary
            .level_counts
            .get(&LogLevel::Error)
            .copied()
            .unwrap_or_default()
            + report
                .summary
                .level_counts
                .get(&LogLevel::Fatal)
                .copied()
                .unwrap_or_default();
        output.push_str(&metric_card("Errors", &error_count.to_string()));
        output.push_str(&metric_card(
            "Sources",
            &report.summary.source_counts.len().to_string(),
        ));
        output.push_str("</section>\n");

        output.push_str("<section><h2>Level Distribution</h2>\n");
        output.push_str("<div class=\"chart-grid\">\n");
        output.push_str(&donut_card(
            "Level share",
            &level_segments(report),
            report.summary.total_count,
        ));
        output.push_str("<div class=\"bar-list\">\n");
        for level in [
            LogLevel::Trace,
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
            LogLevel::Fatal,
        ] {
            let count = report
                .summary
                .level_counts
                .get(&level)
                .copied()
                .unwrap_or_default();
            output.push_str(&bar_row(level.as_str(), count, report.summary.total_count));
        }
        output.push_str("</div>\n</div>\n</section>\n");

        output.push_str("<section><h2>Source Distribution</h2>\n");
        let sources = sorted_sources(report);
        output.push_str("<div class=\"chart-grid\">\n");
        output.push_str(&donut_card(
            "Source share",
            &source_segments(&sources),
            report.summary.total_count,
        ));
        output.push_str("<div class=\"bar-list source-list\">\n");
        for (source, count) in sources {
            output.push_str(&bar_row(source, count, report.summary.total_count));
        }
        output.push_str("</div>\n</div>\n</section>\n");

        for section in &report.sections {
            output.push_str(&format!(
                "<section><h2>{}</h2><pre>{}</pre></section>\n",
                escape_html(&section.heading),
                escape_html(&section.body)
            ));
        }

        output.push_str("</main>\n</body>\n</html>\n");
        Ok(output)
    }
}

/// Inline CSS for the HTML report. Kept as a constant so the output is self-contained.
const HTML_STYLE: &str = r#"<style>
*{box-sizing:border-box}
body{margin:0;background:#f6f8fb;color:#102033;font-family:Segoe UI,Arial,sans-serif}
main{max-width:1180px;margin:0 auto;padding:32px}
h1{margin:0 0 8px;font-size:32px}
h2{margin:0 0 16px;font-size:20px}
section{background:#fff;border:1px solid #d9e1ec;border-radius:8px;margin:16px 0;padding:20px}
.meta{color:#64748b}
.metrics{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:12px;background:transparent;border:0;padding:0}
.metric{background:#fff;border:1px solid #d9e1ec;border-left:5px solid #2563eb;border-radius:8px;padding:16px}
.metric .label{color:#64748b;font-size:13px;text-transform:uppercase}
.metric .value{font-size:28px;font-weight:700;margin-top:6px}
.chart-grid{display:grid;grid-template-columns:minmax(240px,320px) minmax(0,1fr);gap:24px;align-items:start}
.donut-card{border:1px solid #e2e8f0;border-radius:8px;padding:16px;background:#f8fafc}
.donut-title{font-size:13px;color:#64748b;text-transform:uppercase;font-weight:700;margin-bottom:12px}
.donut{width:180px;height:180px;border-radius:50%;margin:0 auto 14px;background:#e5e7eb;box-shadow:inset 0 0 0 28px #fff}
.legend{display:grid;gap:8px;margin-top:10px}
.legend-row{display:grid;grid-template-columns:12px minmax(0,1fr) auto;gap:8px;align-items:center;font-size:13px}
.swatch{width:12px;height:12px;border-radius:3px}
.legend-label{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:#334155}
.legend-count{font-weight:700;color:#0f172a}
.bar-list{display:grid;gap:12px;min-width:0}
.bar-row{display:grid;grid-template-columns:minmax(220px,36%) minmax(180px,1fr) minmax(44px,max-content);gap:14px;align-items:center;min-width:0}
.bar-label{min-width:0;overflow:visible;white-space:normal;overflow-wrap:anywhere;line-height:1.25;color:#0f172a}
.bar{height:14px;background:#e5e7eb;border-radius:999px;overflow:hidden;min-width:120px}
.fill{height:100%;background:linear-gradient(90deg,#38bdf8,#ef4444)}
.bar-count{text-align:right;font-weight:700;color:#0f172a}
pre{white-space:pre-wrap;font-family:Consolas,monospace;background:#0f172a;color:#e2e8f0;border-radius:6px;padding:14px}
@media (max-width:760px){main{padding:18px}.chart-grid{grid-template-columns:1fr}.bar-row{grid-template-columns:1fr}.bar-count{text-align:left}}
</style>
"#;

/// Render a metric card (label + large value) for the top summary row.
fn metric_card(label: &str, value: &str) -> String {
    format!(
        "<article class=\"metric\"><div class=\"label\">{}</div><div class=\"value\">{}</div></article>\n",
        escape_html(label),
        escape_html(value)
    )
}

/// Render a horizontal bar row with label, proportional fill, and count.
fn bar_row(label: &str, count: usize, total: usize) -> String {
    let width = count
        .saturating_mul(100)
        .checked_div(total)
        .unwrap_or_default()
        .min(100);
    format!(
        "<div class=\"bar-row\"><span class=\"bar-label\" title=\"{}\">{}</span><div class=\"bar\"><div class=\"fill\" style=\"width:{}%\"></div></div><strong class=\"bar-count\">{}</strong></div>\n",
        escape_html(label),
        escape_html(label),
        width,
        count
    )
}

/// Build a donut card from a CSS conic-gradient and a color legend.
fn donut_card(title: &str, segments: &[ChartSegment], total: usize) -> String {
    // Build the card from a CSS conic-gradient so the report stays dependency-free.
    let gradient = conic_gradient(segments, total);
    let mut output = format!(
        "<article class=\"donut-card\"><div class=\"donut-title\">{}</div><div class=\"donut\" style=\"background:{}\"></div><div class=\"legend\">\n",
        escape_html(title),
        escape_html(&gradient)
    );
    for segment in segments.iter().filter(|segment| segment.count > 0) {
        let _ = writeln!(
            output,
            "<div class=\"legend-row\"><span class=\"swatch\" style=\"background:{}\"></span><span class=\"legend-label\" title=\"{}\">{}</span><span class=\"legend-count\">{}</span></div>",
            segment.color,
            escape_html(&segment.label),
            escape_html(&segment.label),
            segment.count
        );
    }
    output.push_str("</div></article>\n");
    output
}

/// Convert segment counts into a `conic-gradient(...)` CSS value string.
fn conic_gradient(segments: &[ChartSegment], total: usize) -> String {
    if total == 0 {
        return "#e5e7eb".to_string();
    }

    // Convert counts into percentage slices for the donut background.
    let mut start = 0usize;
    let mut parts = Vec::new();
    for segment in segments.iter().filter(|segment| segment.count > 0) {
        let end = (start + segment.count.saturating_mul(100)).min(total * 100);
        parts.push(format!(
            "{} {:.2}% {:.2}%",
            segment.color,
            start as f64 / total as f64,
            end as f64 / total as f64
        ));
        start = end;
    }

    if parts.is_empty() {
        "#e5e7eb".to_string()
    } else {
        format!("conic-gradient({})", parts.join(","))
    }
}

/// Build chart segments for each log level with a fixed color palette.
fn level_segments(report: &Report) -> Vec<ChartSegment> {
    [
        (LogLevel::Trace, "#94a3b8"),
        (LogLevel::Debug, "#64748b"),
        (LogLevel::Info, "#38bdf8"),
        (LogLevel::Warn, "#f59e0b"),
        (LogLevel::Error, "#ef4444"),
        (LogLevel::Fatal, "#a855f7"),
    ]
    .into_iter()
    .map(|(level, color)| ChartSegment {
        label: level.as_str().to_string(),
        count: report
            .summary
            .level_counts
            .get(&level)
            .copied()
            .unwrap_or_default(),
        color,
    })
    .collect()
}

/// Build chart segments for each source, cycling through an 8-color palette.
fn source_segments(sources: &[(&String, usize)]) -> Vec<ChartSegment> {
    const COLORS: [&str; 8] = [
        "#38bdf8", "#ef4444", "#22c55e", "#f59e0b", "#a855f7", "#14b8a6", "#f97316", "#64748b",
    ];

    sources
        .iter()
        .take(8)
        .enumerate()
        .map(|(index, (source, count))| ChartSegment {
            label: (*source).clone(),
            count: *count,
            color: COLORS[index % COLORS.len()],
        })
        .collect()
}

/// Sort source entries by descending count, then alphabetically for stability.
fn sorted_sources(report: &Report) -> Vec<(&String, usize)> {
    // Keep the most frequent sources first so the report stays stable and readable.
    let mut sources = report
        .summary
        .source_counts
        .iter()
        .map(|(source, count)| (source, *count))
        .collect::<Vec<_>>();
    sources.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    sources
}

/// A slice of a donut chart with a label, count, and CSS color.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChartSegment {
    label: String,
    count: usize,
    color: &'static str,
}

/// Escape `&`, `<`, `>`, and `"` for safe embedding in HTML.
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
