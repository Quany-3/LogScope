use super::app::App;
use crate::model::LogLevel;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

pub(super) fn render_app(frame: &mut Frame<'_>, app: &App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .areas(frame.area());
    let [logs, side] =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).areas(body);
    let [summary, detail, preview] = Layout::vertical([
        Constraint::Length(9),
        Constraint::Length(8),
        Constraint::Min(1),
    ])
    .areas(side);

    frame.render_widget(
        Paragraph::new(format!("LogScope - {}", app.source_label()))
            .alignment(Alignment::Center)
            .style(Style::default().add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL)),
        header,
    );
    frame.render_widget(
        Paragraph::new(styled_log_lines(app))
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
        Paragraph::new(app.selected_entry_details().join("\n"))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title("Selected Entry")
                    .borders(Borders::ALL),
            ),
        detail,
    );
    let (preview_title, preview_lines) = if app.is_file_picker_open() {
        ("Open Log File", app.file_picker_lines())
    } else {
        ("Report Preview", app.report_preview_lines())
    };
    frame.render_widget(
        Paragraph::new(preview_lines.join("\n"))
            .wrap(Wrap { trim: false })
            .block(Block::default().title(preview_title).borders(Borders::ALL)),
        preview,
    );
    frame.render_widget(
        Paragraph::new(Line::from(format!(
            "{} | o open | Space mark | Enter load | q/Esc exit",
            app.status_line()
        )))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL)),
        footer,
    );
}

fn styled_log_lines(app: &App) -> Vec<Line<'static>> {
    if app.entries().is_empty() {
        return vec![Line::from("No log file loaded.")];
    }

    app.entries()
        .iter()
        .take(20)
        .enumerate()
        .map(|(index, entry)| {
            let marker = if app.selected_index() == Some(index) {
                "> "
            } else {
                "  "
            };
            Line::from(vec![
                Span::raw(marker),
                Span::styled(entry.level.to_string(), level_style(entry.level)),
                Span::raw(format!(
                    " {} {} {}",
                    entry.display_timestamp(),
                    entry.source.name,
                    entry.message
                )),
            ])
        })
        .collect()
}

fn level_style(level: LogLevel) -> Style {
    match level {
        LogLevel::Trace | LogLevel::Debug => Style::default().fg(Color::Gray),
        LogLevel::Info => Style::default().fg(Color::Cyan),
        LogLevel::Warn => Style::default().fg(Color::Yellow),
        LogLevel::Error => Style::default().fg(Color::Red),
        LogLevel::Fatal => Style::default().fg(Color::Magenta),
    }
}

#[cfg(test)]
mod tests {
    use super::level_style;
    use crate::model::LogLevel;
    use ratatui::style::{Color, Style};

    #[test]
    fn styles_log_levels_by_urgency() {
        assert_eq!(
            level_style(LogLevel::Info),
            Style::default().fg(Color::Cyan)
        );
        assert_eq!(
            level_style(LogLevel::Warn),
            Style::default().fg(Color::Yellow)
        );
        assert_eq!(
            level_style(LogLevel::Error),
            Style::default().fg(Color::Red)
        );
        assert_eq!(
            level_style(LogLevel::Fatal),
            Style::default().fg(Color::Magenta)
        );
    }
}
