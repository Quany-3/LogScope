use super::{Report, ReportResult, ReportWriter};
use crate::model::LogLevel;

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
        output.push_str("</section>\n");

        output.push_str("<section><h2>Source Distribution</h2>\n");
        let mut sources = report.summary.source_counts.iter().collect::<Vec<_>>();
        sources.sort_by_key(|(source, _)| *source);
        for (source, count) in sources {
            output.push_str(&bar_row(source, *count, report.summary.total_count));
        }
        output.push_str("</section>\n");

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

const HTML_STYLE: &str = r#"<style>
body{margin:0;background:#f6f8fb;color:#1f2937;font-family:Segoe UI,Arial,sans-serif}
main{max-width:1120px;margin:0 auto;padding:32px}
h1{margin:0 0 8px;font-size:32px}
h2{margin:0 0 16px;font-size:20px}
section{background:#fff;border:1px solid #d9e1ec;border-radius:8px;margin:16px 0;padding:20px}
.meta{color:#64748b}
.metrics{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:12px;background:transparent;border:0;padding:0}
.metric{background:#fff;border:1px solid #d9e1ec;border-left:5px solid #2563eb;border-radius:8px;padding:16px}
.metric .label{color:#64748b;font-size:13px;text-transform:uppercase}
.metric .value{font-size:28px;font-weight:700;margin-top:6px}
.bar-row{display:grid;grid-template-columns:120px 1fr 70px;gap:12px;align-items:center;margin:10px 0}
.bar{height:12px;background:#e5e7eb;border-radius:999px;overflow:hidden}
.fill{height:100%;background:linear-gradient(90deg,#38bdf8,#ef4444)}
pre{white-space:pre-wrap;font-family:Consolas,monospace;background:#0f172a;color:#e2e8f0;border-radius:6px;padding:14px}
</style>
"#;

fn metric_card(label: &str, value: &str) -> String {
    format!(
        "<article class=\"metric\"><div class=\"label\">{}</div><div class=\"value\">{}</div></article>\n",
        escape_html(label),
        escape_html(value)
    )
}

fn bar_row(label: &str, count: usize, total: usize) -> String {
    let width = count
        .saturating_mul(100)
        .checked_div(total)
        .unwrap_or_default()
        .min(100);
    format!(
        "<div class=\"bar-row\"><span>{}</span><div class=\"bar\"><div class=\"fill\" style=\"width:{}%\"></div></div><strong>{}</strong></div>\n",
        escape_html(label),
        width,
        count
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
