//! Interactive terminal interface powered by [ratatui] and [crossterm].
//!
//! The TUI displays a scrollable log list, a summary panel, a selected-entry
//! detail view, and a report preview. Keyboard shortcuts allow filtering by
//! level, searching by keyword, opening log files, and exporting HTML reports.

/// Module identifier used for diagnostics and internal logging.
pub const MODULE_NAME: &str = "tui";

mod app;
mod view;

pub use app::App;

use crate::model::LogEntry;
use anyhow::{Context, Result};
use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};
use std::time::Duration;
use view::render_app;

/// Start the interactive terminal application and restore the terminal on exit.
pub fn run() -> Result<()> {
    run_app(App::default())
}

/// Start the TUI with pre-loaded log entries and a source label.
pub fn run_with_entries(source_label: impl Into<String>, entries: Vec<LogEntry>) -> Result<()> {
    run_app(App::from_entries(source_label, entries)?)
}

/// Main event loop: poll for key/resize events, delegate to the app, and redraw.
fn run_app(mut app: App) -> Result<()> {
    let (mut terminal, _guard) = setup_terminal()?;
    terminal.draw(|frame| render_app(frame, &app))?;

    while app.is_running() {
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                app.handle_key_event(key);
                if app.is_running() {
                    terminal.draw(|frame| render_app(frame, &app))?;
                }
            }
            Event::Resize(_, _) => {
                // Resize events need a redraw so the layouts recalculate against the new area.
                terminal.draw(|frame| render_app(frame, &app))?;
            }
            _ => {}
        }
    }

    Ok(())
}

/// Type alias for the ratatui terminal backend used throughout the TUI.
type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Initialize raw mode and the alternate screen, returning a terminal and a
/// guard that restores the original terminal state on drop.
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

/// RAII guard that restores the terminal to its original state (disables raw
/// mode, leaves the alternate screen, and re-shows the cursor) when dropped.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
    }
}
