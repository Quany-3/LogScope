use crate::analyzer::{BasicAnalyzer, OperationalInsights, RealtimeSummary};
use crate::model::{LogEntry, LogTimestamp, ReportMetadata};
use crate::report::{
    MarkdownReportWriter, Report, ReportPreview, ReportSectionBuilder, build_insight_section,
    build_report_preview,
};
use anyhow::{Context, Result};
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent};

/// Runtime state shared by the TUI renderer and keyboard event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    running: bool,
    source_label: String,
    entries: Vec<LogEntry>,
    selected_index: Option<usize>,
    summary: RealtimeSummary,
    insights: Option<OperationalInsights>,
    report_preview: ReportPreview,
    status_message: String,
}

impl App {
    pub fn from_entries(source_label: impl Into<String>, entries: Vec<LogEntry>) -> Result<Self> {
        let source_label = source_label.into();
        let analyzer = BasicAnalyzer;
        let summary = analyzer.realtime_summary(&entries, 10);
        let insights = analyzer.build_insights(&entries, 60, 1_000, 5);
        let report = build_tui_report(&source_label, &entries, &insights);
        let report_preview = build_report_preview(&report, &MarkdownReportWriter, 12)
            .context("failed to build TUI report preview")?;
        let status_message = format!("Loaded {} entries from {source_label}", entries.len());
        let selected_index = (!entries.is_empty()).then_some(0);

        Ok(Self {
            running: true,
            source_label,
            entries,
            selected_index,
            summary,
            insights: Some(insights),
            report_preview,
            status_message,
        })
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn source_label(&self) -> &str {
        &self.source_label
    }

    pub fn summary(&self) -> &RealtimeSummary {
        &self.summary
    }

    pub fn report_preview(&self) -> &ReportPreview {
        &self.report_preview
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub(crate) fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    pub fn log_lines(&self) -> Vec<String> {
        if self.entries.is_empty() {
            return vec!["No log file loaded.".to_string()];
        }

        self.entries
            .iter()
            .take(20)
            .map(LogEntry::display_line)
            .collect()
    }

    pub fn summary_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("Total: {}", self.summary.total_count),
            format!("Warnings: {}", self.summary.warning_count),
            format!("Errors: {}", self.summary.error_count),
        ];

        if let Some(insights) = &self.insights {
            lines.push(format!("Severity: {}/100", insights.severity_score));
            lines.push(format!("Error rate: {}%", insights.error_rate_percent));
            lines.push(format!("Slow rate: {}%", insights.slow_rate_percent));
        }

        if !self.summary.top_sources.is_empty() {
            lines.push("Top sources:".to_string());
            lines.extend(
                self.summary
                    .top_sources
                    .iter()
                    .map(|ranking| format!("{}: {}", ranking.source, ranking.count)),
            );
        }

        lines
    }

    pub fn selected_entry_details(&self) -> Vec<String> {
        let Some(index) = self.selected_index else {
            return vec!["No entry selected.".to_string()];
        };
        let Some(entry) = self.entries.get(index) else {
            return vec!["No entry selected.".to_string()];
        };

        let mut details = vec![
            format!("Timestamp: {}", entry.display_timestamp()),
            format!("Level: {}", entry.level),
            format!("Source: {}", entry.source.name),
            format!("Message: {}", entry.message),
        ];
        for (key, value) in &entry.fields {
            details.push(format!("{key}: {value}"));
        }
        details
    }

    pub fn status_line(&self) -> &str {
        &self.status_message
    }

    pub fn report_preview_lines(&self) -> Vec<String> {
        if self.report_preview.lines.is_empty() {
            return vec!["No report preview available.".to_string()];
        }

        let mut lines = self.report_preview.lines.clone();
        if self.report_preview.truncated {
            lines.push(format!(
                "... {} total lines",
                self.report_preview.total_lines
            ));
        }
        lines
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Down => self.move_selection(1),
            KeyCode::Up => self.move_selection(-1),
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let Some(current) = self.selected_index else {
            return;
        };
        let last = self.entries.len().saturating_sub(1);
        let next = current.saturating_add_signed(delta).min(last);
        self.selected_index = Some(next);
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: true,
            source_label: "No file".to_string(),
            entries: Vec::new(),
            selected_index: None,
            summary: RealtimeSummary::default(),
            insights: None,
            report_preview: ReportPreview::default(),
            status_message: "No log file loaded.".to_string(),
        }
    }
}

fn build_tui_report(
    source_label: &str,
    entries: &[LogEntry],
    insights: &OperationalInsights,
) -> Report {
    let analyzer = BasicAnalyzer;
    let summary = analyzer.build_summary(entries, 5, 1_000);
    let mut source_section = ReportSectionBuilder::new("Top Sources");
    for ranking in &summary.top_sources {
        source_section = source_section.line(format!("{}: {}", ranking.source, ranking.count));
    }

    Report {
        title: "LogScope TUI Preview".to_string(),
        metadata: Some(ReportMetadata {
            generated_at: LogTimestamp::new(Utc::now()),
            source: source_label.to_string(),
            entry_count: entries.len(),
        }),
        summary: summary.basic,
        sections: vec![build_insight_section(insights), source_section.build()],
    }
}

#[cfg(test)]
mod tests {
    use super::App;
    use crate::model::{LogEntry, LogLevel, LogSource, LogTimestamp};
    use chrono::{TimeZone, Utc};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn builds_display_state_from_loaded_entries() {
        let app = App::from_entries("sample.log", sample_entries()).unwrap();

        assert_eq!(app.source_label(), "sample.log");
        assert_eq!(app.summary().total_count, 2);
        assert_eq!(
            app.log_lines()[0],
            "2026-06-12T10:01:00Z ERROR api failed duration_ms=1200"
        );
        assert!(app.summary_lines().contains(&"Errors: 1".to_string()));
        assert!(
            app.summary_lines()
                .iter()
                .any(|line| line.starts_with("Severity: "))
        );
        assert!(app.report_preview().total_lines > 0);
        assert_eq!(app.status_line(), "Loaded 2 entries from sample.log");
    }

    #[test]
    fn initializes_application_state_for_tui_panels() {
        let app = App::default();

        assert!(app.is_running());
        assert_eq!(app.summary().total_count, 0);
        assert!(app.summary().recent_lines.is_empty());
        assert!(app.report_preview().lines.is_empty());
        assert!(!app.report_preview().truncated);
    }

    #[test]
    fn default_state_renders_empty_panel_content() {
        let app = App::default();

        assert_eq!(app.log_lines(), vec!["No log file loaded.".to_string()]);
        assert!(app.summary_lines().contains(&"Total: 0".to_string()));
        assert_eq!(app.status_line(), "No log file loaded.");
    }

    #[test]
    fn default_state_renders_empty_report_preview_content() {
        let app = App::default();

        assert_eq!(
            app.report_preview_lines(),
            vec!["No report preview available.".to_string()]
        );
    }

    #[test]
    fn keyboard_navigation_changes_selected_entry() {
        let mut app = App::from_entries("sample.log", sample_entries()).unwrap();

        assert_eq!(app.selected_index(), Some(0));
        app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected_index(), Some(1));
        app.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.selected_index(), Some(0));
    }

    #[test]
    fn selected_entry_details_include_structured_fields() {
        let app = App::from_entries("sample.log", sample_entries()).unwrap();
        let details = app.selected_entry_details();

        assert!(details.iter().any(|line| line == "Level: ERROR"));
        assert!(details.iter().any(|line| line == "duration_ms: 1200"));
    }

    #[test]
    fn quit_keys_stop_the_application() {
        for key_code in [KeyCode::Char('q'), KeyCode::Esc] {
            let mut app = App::default();

            app.handle_key_event(KeyEvent::new(key_code, KeyModifiers::NONE));

            assert!(!app.is_running());
        }
    }

    #[test]
    fn unrelated_keys_keep_the_application_running() {
        let mut app = App::default();

        app.handle_key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        assert!(app.is_running());
    }

    fn sample_entries() -> Vec<LogEntry> {
        vec![
            sample_entry(LogLevel::Error, "failed duration_ms=1200", 1),
            sample_entry(LogLevel::Info, "recovered", 2),
        ]
    }

    fn sample_entry(level: LogLevel, message: &str, minute: u32) -> LogEntry {
        let timestamp = Utc.with_ymd_and_hms(2026, 6, 12, 10, minute, 0).unwrap();
        LogEntry {
            timestamp: LogTimestamp::new(timestamp),
            level,
            source: LogSource::new("api"),
            fields: message
                .split_whitespace()
                .filter_map(|token| {
                    let (key, value) = token.split_once('=')?;
                    Some((key.to_string(), value.to_string()))
                })
                .collect(),
            raw: format!("{} {} api {}", timestamp.to_rfc3339(), level, message),
            message: message.to_string(),
        }
    }
}
