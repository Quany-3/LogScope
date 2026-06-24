use crate::analyzer::{BasicAnalyzer, OperationalInsights, RealtimeSummary};
use crate::model::{LogEntry, LogTimestamp, ReportMetadata};
use crate::parser::{JsonLineLogParser, PlainTextLogParser, parse_file};
use crate::report::{
    MarkdownReportWriter, Report, ReportPreview, ReportSectionBuilder, build_insight_section,
    build_report_preview,
};
use anyhow::{Context, Result};
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent};
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

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
    file_picker: FilePickerState,
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
            file_picker: FilePickerState::default(),
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

    pub fn is_file_picker_open(&self) -> bool {
        self.file_picker.open
    }

    pub fn open_file_picker_with(&mut self, files: Vec<PathBuf>) {
        self.file_picker = FilePickerState {
            open: true,
            files,
            selected_index: 0,
            marked_indices: BTreeSet::new(),
        };
        if self.file_picker.files.is_empty() {
            self.status_message = "No log files found.".to_string();
        } else {
            self.status_message =
                "Select log files with Space, then press Enter to load.".to_string();
        }
    }

    pub fn file_picker_lines(&self) -> Vec<String> {
        if !self.file_picker.open {
            return Vec::new();
        }
        if self.file_picker.files.is_empty() {
            return vec!["No .log, .json, or .jsonl files found.".to_string()];
        }

        self.file_picker
            .files
            .iter()
            .enumerate()
            .map(|(index, path)| {
                let cursor = if index == self.file_picker.selected_index {
                    ">"
                } else {
                    " "
                };
                let checked = if self.file_picker.marked_indices.contains(&index) {
                    "[x]"
                } else {
                    "[ ]"
                };
                format!("{cursor} {checked} {}", path.display())
            })
            .collect()
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
            KeyCode::Esc if self.file_picker.open => self.file_picker.open = false,
            KeyCode::Char('q') | KeyCode::Esc => self.running = false,
            KeyCode::Char('o') => self.open_file_picker(),
            KeyCode::Char(' ') if self.file_picker.open => self.toggle_file_picker_mark(),
            KeyCode::Enter if self.file_picker.open => self.load_marked_files(),
            KeyCode::Down if self.file_picker.open => self.move_file_picker(1),
            KeyCode::Up if self.file_picker.open => self.move_file_picker(-1),
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

    fn open_file_picker(&mut self) {
        self.open_file_picker_with(discover_log_files());
    }

    fn move_file_picker(&mut self, delta: isize) {
        if self.file_picker.files.is_empty() {
            return;
        }
        let last = self.file_picker.files.len().saturating_sub(1);
        self.file_picker.selected_index = self
            .file_picker
            .selected_index
            .saturating_add_signed(delta)
            .min(last);
    }

    fn toggle_file_picker_mark(&mut self) {
        if self.file_picker.files.is_empty() {
            return;
        }

        let index = self.file_picker.selected_index;
        if !self.file_picker.marked_indices.remove(&index) {
            self.file_picker.marked_indices.insert(index);
        }
        let count = self.file_picker.marked_indices.len();
        self.status_message = format!("{count} file(s) selected. Press Enter to load.");
    }

    fn load_marked_files(&mut self) {
        let paths = self.selected_file_picker_paths();
        if paths.is_empty() {
            return;
        }

        match parse_log_files(&paths)
            .and_then(|entries| Self::from_entries(source_label_for_paths(&paths), entries))
        {
            Ok(next) => *self = next,
            Err(error) => self.status_message = format!("Failed to load selected file(s): {error}"),
        }
    }

    fn selected_file_picker_paths(&self) -> Vec<PathBuf> {
        if self.file_picker.marked_indices.is_empty() {
            return self
                .file_picker
                .files
                .get(self.file_picker.selected_index)
                .cloned()
                .into_iter()
                .collect();
        }

        self.file_picker
            .marked_indices
            .iter()
            .filter_map(|index| self.file_picker.files.get(*index).cloned())
            .collect()
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
            file_picker: FilePickerState::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FilePickerState {
    open: bool,
    files: Vec<PathBuf>,
    selected_index: usize,
    marked_indices: BTreeSet<usize>,
}

fn discover_log_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for directory in [Path::new("samples"), Path::new(".")] {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_supported_log_file(&path) {
                files.push(path);
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

fn is_supported_log_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("log" | "json" | "jsonl")
    )
}

fn parse_log_file(path: &Path) -> Result<Vec<LogEntry>> {
    let mut entries = if should_parse_as_json(path)? {
        parse_file(path, &JsonLineLogParser)?
    } else {
        parse_file(path, &PlainTextLogParser)?
    };
    for entry in &mut entries {
        entry
            .fields
            .insert("origin_file".to_string(), path.display().to_string());
    }
    entries.sort_by_key(|entry| entry.timestamp.value);
    Ok(entries)
}

fn parse_log_files(paths: &[PathBuf]) -> Result<Vec<LogEntry>> {
    let mut entries = Vec::new();
    for path in paths {
        // Each file may be text or JSON, so detection stays per-file.
        entries.extend(
            parse_log_file(path).with_context(|| format!("failed to load {}", path.display()))?,
        );
    }
    entries.sort_by_key(|entry| entry.timestamp.value);
    Ok(entries)
}

fn source_label_for_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn should_parse_as_json(path: &Path) -> Result<bool> {
    if matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("json" | "jsonl")
    ) {
        return Ok(true);
    }

    let file = fs::File::open(path)
        .with_context(|| format!("failed to inspect log file {}", path.display()))?;
    for line in BufReader::new(file).lines() {
        let line = line?;
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        return Ok(trimmed.starts_with('{'));
    }

    Ok(false)
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
    use std::fs;

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

    #[test]
    fn file_picker_loads_selected_log_file() {
        let path = write_temp_log("2026-06-12T10:00:00Z ERROR api failed duration_ms=1200\n");
        let mut app = App::default();
        app.open_file_picker_with(vec![path.clone()]);

        assert!(app.is_file_picker_open());
        assert_eq!(
            app.file_picker_lines(),
            vec![format!("> [ ] {}", path.display())]
        );

        app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        fs::remove_file(&path).unwrap();
        assert!(!app.is_file_picker_open());
        assert_eq!(app.summary().total_count, 1);
        assert_eq!(app.selected_entry_details()[1], "Level: ERROR");
    }

    #[test]
    fn file_picker_loads_marked_log_files_together() {
        let first = write_temp_log("2026-06-12T10:01:00Z ERROR api failed duration_ms=1200\n");
        let second = write_temp_log("2026-06-12T10:00:00Z INFO worker started\n");
        let mut app = App::default();
        app.open_file_picker_with(vec![first.clone(), second.clone()]);

        app.handle_key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert_eq!(
            app.file_picker_lines(),
            vec![
                format!("  [x] {}", first.display()),
                format!("> [x] {}", second.display())
            ]
        );

        app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        fs::remove_file(first).unwrap();
        fs::remove_file(second).unwrap();
        assert_eq!(app.summary().total_count, 2);
        assert!(app.source_label().contains(", "));
        assert_eq!(app.selected_entry_details()[1], "Level: INFO");
    }

    #[test]
    fn file_picker_detects_json_content_with_log_extension() {
        let path = write_temp_log(
            "{\"timestamp\":\"2026-06-12T10:00:00Z\",\"level\":\"INFO\",\"source\":\"api\",\"message\":\"service started\"}\n",
        );
        let mut app = App::default();
        app.open_file_picker_with(vec![path.clone()]);

        app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        fs::remove_file(&path).unwrap();
        assert!(!app.is_file_picker_open());
        assert_eq!(app.summary().total_count, 1);
        assert_eq!(app.selected_entry_details()[1], "Level: INFO");
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

    fn write_temp_log(content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "logscope-picker-{}.log",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, content).unwrap();
        path
    }
}
