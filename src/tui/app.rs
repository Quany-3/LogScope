//! Application state and event handling for the TUI.
//!
//! [`App`] owns the loaded log entries, filter state, summary, insights, and
//! report preview. All keyboard events are routed through [`App::handle_key_event`].

use crate::analyzer::{BasicAnalyzer, OperationalInsights, RealtimeSummary};
use crate::model::{LogEntry, LogLevel, LogTimestamp, ReportMetadata};
use crate::parser::parse_file_auto_with;
use crate::report::{
    HtmlReportWriter, MarkdownReportWriter, Report, ReportPreview, ReportSectionBuilder,
    ReportWriter, build_diagnostic_section, build_insight_section, build_report_preview,
};
use crate::utils::write_file_safely;
use anyhow::{Context, Result};
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// File size threshold above which a log file is considered "large" (50 MiB).
/// Large-file inputs trigger a scrolling-window hint in the TUI.
const LARGE_LOG_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Runtime state shared by the TUI renderer and keyboard event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    /// Whether the event loop should keep running.
    running: bool,
    /// Human-readable label for the loaded log source.
    source_label: String,
    /// All parsed log entries, sorted by timestamp.
    entries: Vec<LogEntry>,
    /// Indices into `entries` that pass the current filter.
    filtered_indices: Vec<usize>,
    /// Cursor position within `filtered_indices` (not the entries slice).
    selected_filtered_index: Option<usize>,
    /// Vertical scroll offset for the log panel.
    log_offset: usize,
    /// Active level and keyword filters.
    filters: LogFilters,
    /// When `Some`, the user is typing a search keyword.
    search_input: Option<String>,
    /// When `true`, the next quit key will actually exit (two-press confirmation).
    quit_pending: bool,
    /// Precomputed summary for the right panel.
    summary: RealtimeSummary,
    /// Precomputed operational insights for the right panel and reports.
    insights: Option<OperationalInsights>,
    /// Truncated markdown preview shown in the bottom-right panel.
    report_preview: ReportPreview,
    /// One-line status message displayed in the footer.
    status_message: String,
    /// State for the file picker overlay.
    file_picker: FilePickerState,
}

impl App {
    /// Build an `App` from pre-parsed entries, precomputing the summary and report preview.
    pub fn from_entries(source_label: impl Into<String>, entries: Vec<LogEntry>) -> Result<Self> {
        let source_label = source_label.into();
        let analyzer = BasicAnalyzer;
        // Precompute the first summary/report snapshot so the TUI can render immediately.
        let summary = analyzer.realtime_summary(&entries, 10);
        let insights = analyzer.build_insights(&entries, 60, 1_000, 5);
        let report = build_tui_report(&source_label, &entries, &insights);
        let report_preview = build_report_preview(&report, &MarkdownReportWriter, 12)
            .context("failed to build TUI report preview")?;
        let status_message = format!("Loaded {} entries from {source_label}", entries.len());
        let filtered_indices = (0..entries.len()).collect::<Vec<_>>();
        let selected_filtered_index = (!filtered_indices.is_empty()).then_some(0);

        Ok(Self {
            running: true,
            source_label,
            entries,
            filtered_indices,
            selected_filtered_index,
            log_offset: 0,
            filters: LogFilters::default(),
            search_input: None,
            quit_pending: false,
            summary,
            insights: Some(insights),
            report_preview,
            status_message,
            file_picker: FilePickerState::default(),
        })
    }

    /// Whether the event loop should continue running.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// The display label for the loaded log source.
    pub fn source_label(&self) -> &str {
        &self.source_label
    }

    /// Precomputed summary for the right panel.
    pub fn summary(&self) -> &RealtimeSummary {
        &self.summary
    }

    /// Truncated markdown report preview for the bottom-right panel.
    pub fn report_preview(&self) -> &ReportPreview {
        &self.report_preview
    }

    /// Absolute index of the selected entry in the `entries` slice, if any.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected_entry_index()
    }

    /// Current vertical scroll offset for the log list.
    pub fn log_offset(&self) -> usize {
        self.log_offset
    }

    /// Read-only access to the full entry list.
    pub(crate) fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Return the entries visible in the log panel, respecting scroll offset and row limit.
    pub(crate) fn visible_log_entries(&self, max_rows: usize) -> Vec<(usize, &LogEntry)> {
        if max_rows == 0 {
            return Vec::new();
        }

        let offset = self.visible_log_offset(max_rows);
        self.filtered_indices
            .iter()
            .skip(offset)
            .take(max_rows)
            .copied()
            .filter_map(|index| self.entries.get(index).map(|entry| (index, entry)))
            .collect()
    }

    /// Human-readable description of the active filter.
    pub fn filter_label(&self) -> String {
        if let Some(search_input) = &self.search_input {
            return format!("searching: {search_input}");
        }
        self.filters.label()
    }

    /// Whether the file picker overlay is currently shown.
    pub fn is_file_picker_open(&self) -> bool {
        self.file_picker.open
    }

    /// Open the file picker overlay with a pre-populated list of log files.
    pub fn open_file_picker_with(&mut self, files: Vec<PathBuf>) {
        self.file_picker = FilePickerState {
            open: true,
            files,
            selected_index: 0,
            marked_indices: BTreeSet::new(),
            current_dir: None,
        };
        if self.file_picker.files.is_empty() {
            self.status_message = "No log files found.".to_string();
        } else {
            self.status_message =
                "Select log files with Space, then press Enter to load.".to_string();
        }
    }

    /// Open the file picker overlay rooted at the given directory.
    pub fn open_file_picker_at(&mut self, directory: PathBuf) {
        self.file_picker = FilePickerState {
            open: true,
            files: discover_file_picker_entries(&directory),
            selected_index: 0,
            marked_indices: BTreeSet::new(),
            current_dir: Some(directory.clone()),
        };
        if self.file_picker.files.is_empty() {
            self.status_message =
                format!("No log files or directories in {}.", directory.display());
        } else {
            self.status_message = format!("Browsing {}.", directory.display());
        }
    }

    /// Rendered lines for the file picker overlay.
    pub fn file_picker_lines(&self) -> Vec<String> {
        if !self.file_picker.open {
            return Vec::new();
        }
        if self.file_picker.files.is_empty() {
            return vec!["No directories, .log, .json, or .jsonl files found.".to_string()];
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
                let checked = if path.is_dir() {
                    "[dir]"
                } else if self.file_picker.marked_indices.contains(&index) {
                    "[x]"
                } else {
                    "[ ]"
                };
                format!("{cursor} {checked} {}", path.display())
            })
            .collect()
    }

    /// The log lines visible in the log panel (up to 20 rows from the current offset).
    pub fn log_lines(&self) -> Vec<String> {
        if self.filtered_indices.is_empty() {
            return vec!["No log file loaded.".to_string()];
        }

        self.filtered_indices
            .iter()
            .skip(self.log_offset)
            .take(20)
            .filter_map(|index| self.entries.get(*index))
            .map(LogEntry::display_line)
            .collect()
    }

    /// Lines for the summary panel, recomputed from the filtered subset.
    pub fn summary_lines(&self) -> Vec<String> {
        let filtered_entries = self.filtered_entries();
        let analyzer = BasicAnalyzer;
        // Recompute from the filtered subset so the side panel always matches what is visible.
        let summary = analyzer.realtime_summary(&filtered_entries, 10);
        let insights = analyzer.build_insights(&filtered_entries, 60, 1_000, 5);
        let mut lines = vec![
            format!("Total: {}", summary.total_count),
            format!("Warnings: {}", summary.warning_count),
            format!("Errors: {}", summary.error_count),
            format!("Filter: {}", self.filter_label()),
        ];

        lines.push(format!("Severity: {}/100", insights.severity_score));
        lines.push(format!("Error rate: {}%", insights.error_rate_percent));
        lines.push(format!("Slow rate: {}%", insights.slow_rate_percent));
        if !insights.file_activity.is_empty() {
            lines.push("Top files:".to_string());
            lines.extend(insights.file_activity.iter().take(3).map(|file| {
                format!(
                    "{}: severity {}/100, errors {}/{}",
                    file.path, file.severity_score, file.error_count, file.total_count
                )
            }));
        }

        if !summary.top_sources.is_empty() {
            lines.push("Top sources:".to_string());
            lines.extend(
                summary
                    .top_sources
                    .iter()
                    .map(|ranking| format!("{}: {}", ranking.source, ranking.count)),
            );
        }

        lines
    }

    /// Formatted key-value lines for the selected entry detail panel.
    pub fn selected_entry_details(&self) -> Vec<String> {
        let Some(index) = self.selected_entry_index() else {
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

    /// The current one-line status message for the footer.
    pub fn status_line(&self) -> &str {
        &self.status_message
    }

    /// Contextual keyboard hint line for the footer.
    pub fn hint_line(&self) -> String {
        if let Some(input) = &self.search_input {
            return format!("Search: {input}_ | Enter apply | Esc cancel | Backspace delete");
        }
        if self.quit_pending {
            return "Quit confirmation: press q/Esc again to exit, any other key cancels."
                .to_string();
        }
        if self.file_picker.open {
            return "File picker: Enter open/load | Space mark file | Backspace parent | Esc close"
                .to_string();
        }
        "Keys: / search | e ERROR+ | w WARN+ | c clear | o open | r export HTML | q/Esc quit"
            .to_string()
    }

    /// Report preview lines, with a truncation notice when the full report is longer.
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

    /// Dispatch a key press to the appropriate handler (search, filter, navigation, etc.).
    pub fn handle_key_event(&mut self, key: KeyEvent) {
        if self.handle_search_key_event(key) {
            return;
        }

        if self.quit_pending && !matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
            self.cancel_quit_confirmation();
        }

        match key.code {
            KeyCode::Esc if self.file_picker.open => self.file_picker.open = false,
            KeyCode::Char('q') | KeyCode::Esc => self.confirm_or_request_quit(),
            KeyCode::Char('o') => self.open_file_picker(),
            KeyCode::Char('/') => self.start_search(),
            KeyCode::Char('e') => self.set_min_level_filter(Some(LogLevel::Error)),
            KeyCode::Char('w') => self.set_min_level_filter(Some(LogLevel::Warn)),
            KeyCode::Char('c') => self.clear_filters(),
            KeyCode::Char('r') => self.export_html_report(),
            KeyCode::Char(' ') if self.file_picker.open => self.toggle_file_picker_mark(),
            KeyCode::Enter if self.file_picker.open => self.activate_file_picker_selection(),
            KeyCode::Backspace if self.file_picker.open => self.open_parent_directory(),
            KeyCode::Down if self.file_picker.open => self.move_file_picker(1),
            KeyCode::Up if self.file_picker.open => self.move_file_picker(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Up => self.move_selection(-1),
            _ => self.cancel_quit_confirmation(),
        }
    }

    fn confirm_or_request_quit(&mut self) {
        if self.quit_pending {
            self.running = false;
        } else {
            self.quit_pending = true;
            self.status_message = "Press q again to quit.".to_string();
        }
    }

    fn cancel_quit_confirmation(&mut self) {
        if self.quit_pending {
            self.quit_pending = false;
            self.status_message = "Quit cancelled.".to_string();
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let Some(current) = self.selected_filtered_index else {
            return;
        };
        let last = self.filtered_indices.len().saturating_sub(1);
        let next = current.saturating_add_signed(delta).min(last);
        self.selected_filtered_index = Some(next);
        self.keep_selected_log_visible(20);
    }

    fn visible_log_offset(&self, max_rows: usize) -> usize {
        let Some(selected_index) = self.selected_filtered_index else {
            return self.log_offset;
        };
        // Keep the selected row inside the viewport instead of jumping the window every keypress.
        if self.filtered_indices.len() <= max_rows {
            return 0;
        }
        if selected_index < self.log_offset {
            return selected_index;
        }
        if selected_index >= self.log_offset + max_rows {
            return selected_index + 1 - max_rows;
        }
        self.log_offset
    }

    fn keep_selected_log_visible(&mut self, max_rows: usize) {
        self.log_offset = self.visible_log_offset(max_rows);
    }

    fn selected_entry_index(&self) -> Option<usize> {
        self.selected_filtered_index
            .and_then(|index| self.filtered_indices.get(index).copied())
    }

    fn filtered_entries(&self) -> Vec<LogEntry> {
        self.filtered_indices
            .iter()
            .filter_map(|index| self.entries.get(*index).cloned())
            .collect()
    }

    fn start_search(&mut self) {
        self.quit_pending = false;
        self.search_input = Some(String::new());
        self.status_message = "Search logs: type keyword, Enter apply, Esc cancel.".to_string();
    }

    fn handle_search_key_event(&mut self, key: KeyEvent) -> bool {
        let Some(input) = self.search_input.as_mut() else {
            return false;
        };

        match key.code {
            KeyCode::Enter => {
                let keyword = input.trim().to_string();
                self.search_input = None;
                self.filters.keyword = (!keyword.is_empty()).then_some(keyword);
                self.apply_filters();
            }
            KeyCode::Esc => {
                self.search_input = None;
                self.status_message = "Search cancelled.".to_string();
            }
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Char(character) => {
                input.push(character);
            }
            _ => {}
        }
        true
    }

    fn set_min_level_filter(&mut self, min_level: Option<LogLevel>) {
        self.filters.min_level = min_level;
        self.apply_filters();
    }

    fn clear_filters(&mut self) {
        self.filters = LogFilters::default();
        self.search_input = None;
        self.apply_filters();
    }

    fn apply_filters(&mut self) {
        // Store indexes rather than cloning entries so filtering stays cheap on large inputs.
        self.filtered_indices = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| self.filters.matches(entry).then_some(index))
            .collect();
        self.selected_filtered_index = (!self.filtered_indices.is_empty()).then_some(0);
        self.log_offset = 0;
        self.status_message = format!(
            "{} of {} entries visible.",
            self.filtered_indices.len(),
            self.entries.len()
        );
    }

    fn export_html_report(&mut self) {
        if self.entries.is_empty() {
            self.status_message = "No log entries to export.".to_string();
            return;
        }

        let analyzer = BasicAnalyzer;
        // Export always uses the full loaded dataset so the report is not scoped by transient TUI filters.
        let insights = analyzer.build_insights(&self.entries, 60, 1_000, 5);
        let report = build_tui_report(&self.source_label, &self.entries, &insights);
        match HtmlReportWriter
            .write(&report)
            .context("failed to render HTML report")
            .and_then(|content| {
                write_file_safely("logscope-tui-report.html", &content)
                    .context("failed to write HTML report")
            }) {
            Ok(()) => {
                self.status_message =
                    "Exported HTML report to logscope-tui-report.html.".to_string();
            }
            Err(error) => {
                self.status_message = format!("Failed to export HTML report: {error}");
            }
        }
    }

    fn open_file_picker(&mut self) {
        self.open_file_picker_at(PathBuf::from("."));
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
        if self
            .file_picker
            .files
            .get(index)
            .is_some_and(|path| path.is_dir())
        {
            self.status_message = "Directories cannot be selected.".to_string();
            return;
        }

        if !self.file_picker.marked_indices.remove(&index) {
            self.file_picker.marked_indices.insert(index);
        }
        let count = self.file_picker.marked_indices.len();
        self.status_message = format!("{count} file(s) selected. Press Enter to load.");
    }

    fn activate_file_picker_selection(&mut self) {
        let Some(path) = self
            .file_picker
            .files
            .get(self.file_picker.selected_index)
            .cloned()
        else {
            return;
        };

        if path.is_dir() {
            self.open_file_picker_at(path);
        } else {
            self.load_marked_files();
        }
    }

    fn open_parent_directory(&mut self) {
        let Some(current_dir) = self.file_picker.current_dir.as_ref() else {
            return;
        };
        let Some(parent) = current_dir.parent() else {
            return;
        };

        self.open_file_picker_at(parent.to_path_buf());
    }

    fn load_marked_files(&mut self) {
        let paths = self.selected_file_picker_paths();
        if paths.is_empty() {
            return;
        }
        let has_large_file = has_large_log_file(&paths);

        // Replace the whole app state so selection, summaries, and preview stay consistent.
        match parse_log_files(&paths)
            .and_then(|entries| Self::from_entries(source_label_for_paths(&paths), entries))
        {
            Ok(mut next) => {
                if has_large_file {
                    next.status_message = format!(
                        "Loaded {} entries from large log input; rendering uses a scroll window.",
                        next.entries.len()
                    );
                }
                *self = next;
            }
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
            .filter_map(|index| self.file_picker.files.get(*index))
            .filter(|path| path.is_file())
            .cloned()
            .collect()
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: true,
            source_label: "No file".to_string(),
            entries: Vec::new(),
            filtered_indices: Vec::new(),
            selected_filtered_index: None,
            log_offset: 0,
            filters: LogFilters::default(),
            search_input: None,
            quit_pending: false,
            summary: RealtimeSummary::default(),
            insights: None,
            report_preview: ReportPreview::default(),
            status_message: "No log file loaded.".to_string(),
            file_picker: FilePickerState::default(),
        }
    }
}

/// Combined level and keyword filter state for the TUI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct LogFilters {
    min_level: Option<LogLevel>,
    keyword: Option<String>,
}

impl LogFilters {
    /// Return `true` when the entry satisfies both the minimum-level and keyword constraints.
    fn matches(&self, entry: &LogEntry) -> bool {
        if let Some(min_level) = self.min_level
            && !level_at_least(entry.level, min_level)
        {
            return false;
        }
        if let Some(keyword) = &self.keyword {
            // Search both raw and normalized message text so structured and plain logs behave the same.
            let keyword = keyword.to_ascii_lowercase();
            let raw = entry.raw.to_ascii_lowercase();
            let message = entry.message.to_ascii_lowercase();
            if !raw.contains(&keyword) && !message.contains(&keyword) {
                return false;
            }
        }
        true
    }

    /// Human-readable label describing the active filters.
    fn label(&self) -> String {
        match (self.min_level, self.keyword.as_deref()) {
            (Some(LogLevel::Error), Some(keyword)) => format!("ERROR+ search: {keyword}"),
            (Some(LogLevel::Warn), Some(keyword)) => format!("WARN+ search: {keyword}"),
            (_, Some(keyword)) => format!("search: {keyword}"),
            (Some(LogLevel::Error), None) => "ERROR+".to_string(),
            (Some(LogLevel::Warn), None) => "WARN+".to_string(),
            _ => "All".to_string(),
        }
    }
}

/// Check whether `level` is at least `min_level` using numeric rank comparison.
fn level_at_least(level: LogLevel, min_level: LogLevel) -> bool {
    level_rank(level) >= level_rank(min_level)
}

/// Assign a numeric rank to each level for minimum-level comparisons.
fn level_rank(level: LogLevel) -> u8 {
    match level {
        LogLevel::Trace => 0,
        LogLevel::Debug => 1,
        LogLevel::Info => 2,
        LogLevel::Warn => 3,
        LogLevel::Error => 4,
        LogLevel::Fatal => 5,
    }
}

/// State for the file picker overlay: list of files, cursor, and marked selections.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FilePickerState {
    open: bool,
    files: Vec<PathBuf>,
    selected_index: usize,
    marked_indices: BTreeSet<usize>,
    current_dir: Option<PathBuf>,
}

/// Scan a directory for supported log files and subdirectories.
fn discover_file_picker_entries(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return files;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() || (path.is_file() && is_supported_log_file(&path)) {
            files.push(path);
        }
    }
    // Directories first keeps navigation predictable in mixed folders.
    files.sort_by_key(|path| (!path.is_dir(), path.display().to_string()));
    files.dedup();
    files
}

/// Check whether a file extension is `.log`, `.json`, or `.jsonl`.
fn is_supported_log_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("log" | "json" | "jsonl")
    )
}

/// Parse a single log file, tagging each entry with its `origin_file` field.
fn parse_log_file(path: &Path) -> Result<Vec<LogEntry>> {
    let mut entries = Vec::new();
    parse_file_auto_with(path, |mut entry| {
        // Tag each entry with its source file before cross-file sorting merges them.
        entry
            .fields
            .insert("origin_file".to_string(), path.display().to_string());
        entries.push(entry);
    })?;
    entries.sort_by_key(|entry| entry.timestamp.value);
    Ok(entries)
}

/// Parse multiple log files and merge the results sorted by timestamp.
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

/// Build a comma-separated label from a list of file paths.
fn source_label_for_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Return `true` if any of the given paths exceeds the large-file threshold.
fn has_large_log_file(paths: &[PathBuf]) -> bool {
    paths
        .iter()
        .any(|path| is_large_log_file_with_threshold(path, LARGE_LOG_FILE_BYTES))
}

/// Check whether a single file meets or exceeds the byte threshold.
fn is_large_log_file_with_threshold(path: &Path, threshold: u64) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.len() >= threshold)
        .unwrap_or(false)
}

/// Assemble a [`Report`] for the TUI preview panel from the current entries and insights.
fn build_tui_report(
    source_label: &str,
    entries: &[LogEntry],
    insights: &OperationalInsights,
) -> Report {
    let analyzer = BasicAnalyzer;
    // Keep the TUI preview compact while reusing the same report pipeline as CLI export.
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
        sections: vec![
            build_diagnostic_section(insights),
            build_insight_section(insights),
            source_section.build(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::App;
    use crate::model::{LogEntry, LogLevel, LogSource, LogTimestamp};
    use chrono::{TimeZone, Utc};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::fs;
    use std::fs::OpenOptions;

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
    fn log_window_follows_selected_entry() {
        let mut app = App::from_entries("sample.log", numbered_entries(25)).unwrap();

        for _ in 0..24 {
            app.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }

        assert_eq!(app.selected_index(), Some(24));
        assert_eq!(app.log_offset(), 5);
        assert_eq!(app.visible_log_entries(20)[0].0, 5);
        assert_eq!(app.visible_log_entries(20)[19].0, 24);
    }

    #[test]
    fn selected_entry_details_include_structured_fields() {
        let app = App::from_entries("sample.log", sample_entries()).unwrap();
        let details = app.selected_entry_details();

        assert!(details.iter().any(|line| line == "Level: ERROR"));
        assert!(details.iter().any(|line| line == "duration_ms: 1200"));
    }

    #[test]
    fn summary_lines_include_file_activity() {
        let mut entries = sample_entries();
        entries[0]
            .fields
            .insert("origin_file".to_string(), "api.log".to_string());
        entries[1]
            .fields
            .insert("origin_file".to_string(), "worker.log".to_string());
        let app = App::from_entries("api.log, worker.log", entries).unwrap();

        assert!(app.summary_lines().contains(&"Top files:".to_string()));
        assert!(
            app.summary_lines()
                .iter()
                .any(|line| line == "api.log: severity 100/100, errors 1/1")
        );
    }

    #[test]
    fn detects_large_log_files_by_size_threshold() {
        let path = write_temp_log("2026-06-12T10:00:00Z INFO api started\n");
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(128)
            .unwrap();

        assert!(super::is_large_log_file_with_threshold(&path, 64));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn filters_logs_by_error_and_warning_level() {
        let mut app = App::from_entries("sample.log", mixed_level_entries()).unwrap();

        app.handle_key_event(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(app.filter_label(), "ERROR+");
        assert_eq!(app.summary_lines()[0], "Total: 2");
        assert!(
            app.log_lines()
                .iter()
                .all(|line| line.contains("ERROR") || line.contains("FATAL"))
        );

        app.handle_key_event(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
        assert_eq!(app.filter_label(), "WARN+");
        assert_eq!(app.summary_lines()[0], "Total: 3");
        assert!(
            app.log_lines().iter().all(|line| line.contains("WARN")
                || line.contains("ERROR")
                || line.contains("FATAL"))
        );

        app.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        assert_eq!(app.filter_label(), "All");
        assert_eq!(app.summary_lines()[0], "Total: 4");
    }

    #[test]
    fn filters_logs_by_search_keyword() {
        let mut app = App::from_entries("sample.log", mixed_level_entries()).unwrap();

        app.handle_key_event(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.filter_label(), "search: timeout");
        assert_eq!(app.summary_lines()[0], "Total: 1");
        assert_eq!(app.selected_entry_details()[3], "Message: database timeout");
    }

    #[test]
    fn shows_search_input_hint_while_typing() {
        let mut app = App::from_entries("sample.log", mixed_level_entries()).unwrap();

        app.handle_key_event(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));

        assert_eq!(
            app.hint_line(),
            "Search: db_ | Enter apply | Esc cancel | Backspace delete"
        );
        assert_eq!(app.filter_label(), "searching: db");
    }

    #[test]
    fn quit_requires_confirmation() {
        let mut app = App::default();

        app.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.is_running());
        assert_eq!(app.status_line(), "Press q again to quit.");

        app.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(!app.is_running());
    }

    #[test]
    fn non_quit_key_cancels_quit_confirmation() {
        let mut app = App::default();

        app.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        app.handle_key_event(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));

        assert!(app.is_running());
        assert_eq!(
            app.hint_line(),
            "Search: _ | Enter apply | Esc cancel | Backspace delete"
        );
    }

    #[test]
    fn quit_keys_stop_the_application() {
        for key_code in [KeyCode::Char('q'), KeyCode::Esc] {
            let mut app = App::default();

            app.handle_key_event(KeyEvent::new(key_code, KeyModifiers::NONE));
            assert!(app.is_running());
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
    fn file_picker_enters_directories_and_returns_to_parent() {
        let root = write_temp_dir();
        let nested = root.join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(
            nested.join("worker.log"),
            "2026-06-12T10:00:00Z INFO worker started\n",
        )
        .unwrap();
        let mut app = App::default();

        app.open_file_picker_at(root.clone());
        assert_eq!(
            app.file_picker_lines(),
            vec![format!("> [dir] {}", nested.display())]
        );

        app.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            app.file_picker_lines(),
            vec![format!("> [ ] {}", nested.join("worker.log").display())]
        );

        app.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(
            app.file_picker_lines(),
            vec![format!("> [dir] {}", nested.display())]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_picker_marks_only_log_files_not_directories() {
        let root = write_temp_dir();
        let nested = root.join("nested");
        fs::create_dir(&nested).unwrap();
        let mut app = App::default();

        app.open_file_picker_at(root.clone());
        app.handle_key_event(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

        assert_eq!(
            app.file_picker_lines(),
            vec![format!("> [dir] {}", nested.display())]
        );
        assert_eq!(app.status_line(), "Directories cannot be selected.");

        fs::remove_dir_all(root).unwrap();
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

    #[test]
    fn exports_html_report_from_tui_shortcut() {
        let root = write_temp_dir();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let mut app = App::from_entries("sample.log", sample_entries()).unwrap();

        app.handle_key_event(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));

        let output = fs::read_to_string(root.join("logscope-tui-report.html")).unwrap();
        std::env::set_current_dir(original_dir).unwrap();
        fs::remove_dir_all(root).unwrap();
        assert!(output.contains("<!doctype html>"));
        assert!(app.status_line().contains("Exported HTML report"));
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

    fn numbered_entries(count: usize) -> Vec<LogEntry> {
        (0..count)
            .map(|index| sample_entry(LogLevel::Info, &format!("entry={index}"), index as u32))
            .collect()
    }

    fn mixed_level_entries() -> Vec<LogEntry> {
        vec![
            sample_entry(LogLevel::Info, "started", 1),
            sample_entry(LogLevel::Warn, "retrying", 2),
            sample_entry(LogLevel::Error, "database timeout", 3),
            sample_entry(LogLevel::Fatal, "worker crashed", 4),
        ]
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

    fn write_temp_dir() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "logscope-picker-dir-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        path
    }
}
