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

pub fn run_with_entries(source_label: impl Into<String>, entries: Vec<LogEntry>) -> Result<()> {
    run_app(App::from_entries(source_label, entries)?)
}

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
                terminal.draw(|frame| render_app(frame, &app))?;
            }
            _ => {}
        }
    }

    Ok(())
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
