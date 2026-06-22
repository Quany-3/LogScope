pub const MODULE_NAME: &str = "tui";

use anyhow::{Context, Result};
use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use std::io::{self, Stdout};
use std::time::Duration;

/// Runtime state shared by the TUI renderer and keyboard event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    running: bool,
}

impl App {
    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
            self.running = false;
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self { running: true }
    }
}

/// Start the interactive terminal application and restore the terminal on exit.
pub fn run() -> Result<()> {
    let (mut terminal, _guard) = setup_terminal()?;
    let mut app = App::default();

    while app.is_running() {
        terminal.draw(|frame| {
            let [header, content, footer] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .areas(frame.area());

            frame.render_widget(
                Paragraph::new("LogScope")
                    .alignment(Alignment::Center)
                    .style(Style::default().add_modifier(Modifier::BOLD))
                    .block(Block::default().borders(Borders::ALL)),
                header,
            );
            frame.render_widget(
                Paragraph::new("Waiting for log data...")
                    .block(Block::default().title("Logs").borders(Borders::ALL)),
                content,
            );
            frame.render_widget(
                Paragraph::new(Line::from("Press q or Esc to exit"))
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL)),
                footer,
            );
        })?;

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key_event(key);
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

#[cfg(test)]
mod tests {
    use super::App;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
}
