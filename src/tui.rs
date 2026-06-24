pub const MODULE_NAME: &str = "tui";

use crate::analyzer::{BasicAnalyzer, RealtimeSummary};
use crate::model::{LogEntry, LogTimestamp, ReportMetadata};
use crate::report::{
    MarkdownReportWriter, Report, ReportPreview, ReportSectionBuilder, build_report_preview,
};
use anyhow::{Context, Result};
use chrono::Utc;
use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use std::io::{self, Stdout};
use std::time::Duration;

/// Runtime state shared by the TUI renderer and keyboard event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    running: bool,
    source_label: String,
    entries: Vec<LogEntry>,
    summary: RealtimeSummary,
    report_preview: ReportPreview,
    status_message: String,
}

impl App {
    pub fn from_entries(source_label: impl Into<String>, entries: Vec<LogEntry>) -> Result<Self> {
        let source_label = source_label.into();
        let summary = BasicAnalyzer.realtime_summary(&entries, 10);
        let report = build_tui_report(&source_label, &entries);
        let report_preview = build_report_preview(&report, &MarkdownReportWriter, 12)
            .context("failed to build TUI report preview")?;
        let status_message = format!("Loaded {} entries from {source_label}", entries.len());

        Ok(Self {
            running: true,
            source_label,
            entries,
            summary,
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
        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
            self.running = false;
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: true,
            source_label: "No file".to_string(),
            entries: Vec::new(),
            summary: RealtimeSummary::default(),
            report_preview: ReportPreview::default(),
            status_message: "No log file loaded.".to_string(),
        }
    }
}

/// Start the interactive terminal application and restore the terminal on exit.
pub fn run() -> Result<()> {
    run_app(App::default())
}

pub fn run_with_entries(source_label: impl Into<String>, entries: Vec<LogEntry>) -> Result<()> {
    run_app(App::from_entries(source_label, entries)?)
}

fn run_app(mut app: App) -> Result<()> {
    let (mut terminal, _guard) = setup_terminal()?;

    while app.is_running() {
        terminal.draw(|frame| render_app(frame, &app))?;

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key_event(key);
        }
    }

    Ok(())
}

fn render_app(frame: &mut Frame<'_>, app: &App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .areas(frame.area());
    let [logs, side] =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).areas(body);
    let [summary, preview] =
        Layout::vertical([Constraint::Length(9), Constraint::Min(1)]).areas(side);

    frame.render_widget(
        Paragraph::new(format!("LogScope - {}", app.source_label()))
            .alignment(Alignment::Center)
            .style(Style::default().add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL)),
        header,
    );
    frame.render_widget(
        Paragraph::new(app.log_lines().join("\n"))
            .wrap(Wrap { trim: false })
            .block(Block::default().title("Logs").borders(Borders::ALL)),
        logs,
    );
    frame.render_widget(
        Paragraph::new(app.summary_lines().join("\n"))
            .wrap(Wrap { trim: false })
            .block(Block::default().title("Summary").borders(Borders::ALL)),
        summary,
    );
    frame.render_widget(
        Paragraph::new(app.report_preview_lines().join("\n"))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title("Report Preview")
                    .borders(Borders::ALL),
            ),
        preview,
    );
    frame.render_widget(
        Paragraph::new(Line::from(format!(
            "{} | Press q or Esc to exit",
            app.status_line()
        )))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL)),
        footer,
    );
}

type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

fn setup_terminal() -> Result<(TuiTerminal, TerminalGuard)> {
    enable_raw_mode().context("failed to enable terminal raw mode")?;
    // Create the guard before later setup steps so partial initialization is reversible.
    let guard = TerminalGuard;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;

    let terminal =
        Terminal::new(CrosstermBackend::new(stdout)).context("failed to initialize terminal")?;
    Ok((terminal, guard))
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
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

        app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

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
            fields: Default::default(),
            raw: format!("{} {} api {}", timestamp.to_rfc3339(), level, message),
            message: message.to_string(),
        }
    }
}

fn build_tui_report(source_label: &str, entries: &[LogEntry]) -> Report {
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
        sections: vec![source_section.build()],
    }
}
