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
        Constraint::Length(4),
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
        Paragraph::new(styled_log_lines(
            app,
            logs.height.saturating_sub(2) as usize,
        ))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(format!("Logs [{}]", app.filter_label()))
                .borders(Borders::ALL),
        ),
        logs,
    );
    frame.render_widget(
        Paragraph::new(styled_summary_lines(app))
            .wrap(Wrap { trim: false })
            .block(Block::default().title("Summary").borders(Borders::ALL)),
        summary,
    );
    frame.render_widget(
        Paragraph::new(styled_selected_entry_details(app))
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
        Paragraph::new(styled_preview_lines(preview_lines))
            .wrap(Wrap { trim: false })
            .block(Block::default().title(preview_title).borders(Borders::ALL)),
        preview,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                app.status_line().to_string(),
                Style::default().fg(Color::Cyan),
            )),
            Line::from(Span::styled(
                app.hint_line(),
                Style::default().fg(Color::Gray),
            )),
        ])
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL)),
        footer,
    );
}

fn styled_log_lines(app: &App, max_rows: usize) -> Vec<Line<'static>> {
    if app.entries().is_empty() {
        return vec![Line::from("No log file loaded.")];
    }

    let visible_entries = app.visible_log_entries(max_rows);
    if visible_entries.is_empty() {
        return vec![Line::from("No logs match the active filter.")];
    }

    visible_entries
        .into_iter()
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

fn styled_summary_lines(app: &App) -> Vec<Line<'static>> {
    app.summary_lines()
        .into_iter()
        .map(|line| Line::from(Span::styled(line.clone(), summary_style(&line))))
        .collect()
}

fn styled_selected_entry_details(app: &App) -> Vec<Line<'static>> {
    app.selected_entry_details()
        .into_iter()
        .map(styled_detail_line)
        .collect()
}

fn styled_detail_line(line: String) -> Line<'static> {
    let Some((label, value)) = line.split_once(": ") else {
        return Line::from(Span::styled(line, Style::default().fg(Color::Gray)));
    };

    let value_style = if label == "Level" {
        LogLevel::from_label(value).map_or(Style::default(), level_style)
    } else {
        detail_value_style(label)
    };

    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), value_style),
    ])
}

fn detail_value_style(label: &str) -> Style {
    match label {
        "Timestamp" => Style::default().fg(Color::Blue),
        "Source" => Style::default().fg(Color::LightCyan),
        "Message" => Style::default(),
        "duration_ms" => Style::default().fg(Color::Yellow),
        "status" => Style::default().fg(Color::Magenta),
        _ => Style::default().fg(Color::Gray),
    }
}

fn styled_preview_lines(lines: Vec<String>) -> Vec<Line<'static>> {
    lines.into_iter().map(styled_preview_line).collect()
}

fn styled_preview_line(line: String) -> Line<'static> {
    if line.starts_with("# ") {
        return Line::from(Span::styled(
            line,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if line.starts_with("## ") {
        return Line::from(Span::styled(
            line,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if let Some((level, count)) = line
        .strip_prefix("- ")
        .and_then(|item| item.split_once(": "))
        && let Some(level) = LogLevel::from_label(level)
    {
        return Line::from(vec![
            Span::raw("- "),
            Span::styled(level.to_string(), level_style(level)),
            Span::raw(": "),
            Span::styled(count.to_string(), Style::default().fg(Color::LightGreen)),
        ]);
    }
    if let Some((label, value)) = line.split_once(": ") {
        return Line::from(vec![
            Span::styled(
                format!("{label}: "),
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(value.to_string(), preview_value_style(label)),
        ]);
    }
    if line.starts_with("... ") {
        return Line::from(Span::styled(line, Style::default().fg(Color::Gray)));
    }

    Line::from(line)
}

fn preview_value_style(label: &str) -> Style {
    match label {
        "Total entries" => Style::default()
            .fg(Color::LightGreen)
            .add_modifier(Modifier::BOLD),
        "Source" | "Generated at" => Style::default().fg(Color::Gray),
        _ => Style::default(),
    }
}

fn summary_style(line: &str) -> Style {
    if line.starts_with("Errors:") || line.contains("errors ") {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if line.starts_with("Warnings:") {
        Style::default().fg(Color::Yellow)
    } else if line.starts_with("Severity:") {
        severity_style(line)
    } else if line.starts_with("Error rate:") {
        rate_style(line, Color::Red)
    } else if line.starts_with("Slow rate:") {
        rate_style(line, Color::Yellow)
    } else if line.starts_with("Filter:") {
        Style::default().fg(Color::Cyan)
    } else if line.ends_with(':') {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn severity_style(line: &str) -> Style {
    let value = line
        .strip_prefix("Severity: ")
        .and_then(|value| value.split('/').next())
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or_default();

    match value {
        70..=100 => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        35..=69 => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::Green),
    }
}

fn rate_style(line: &str, active_color: Color) -> Style {
    let value = line
        .split_once(':')
        .and_then(|(_, value)| value.trim().strip_suffix('%'))
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or_default();

    if value > 0 {
        Style::default()
            .fg(active_color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    }
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
    use super::{level_style, render_app, styled_detail_line, styled_preview_line, summary_style};
    use crate::model::LogLevel;
    use crate::tui::app::App;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::{Color, Modifier, Style};

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

    #[test]
    fn styles_summary_counts_by_urgency() {
        assert_eq!(
            summary_style("Warnings: 2"),
            Style::default().fg(Color::Yellow)
        );
        assert_eq!(
            summary_style("Errors: 1"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        );
        assert_eq!(
            summary_style("Severity: 80/100"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        );
    }

    #[test]
    fn styles_selected_entry_details_by_field_type() {
        let line = styled_detail_line("Level: ERROR".to_string());

        assert_eq!(line.spans[0].style.fg, Some(Color::Cyan));
        assert_eq!(line.spans[1].style.fg, Some(Color::Red));
    }

    #[test]
    fn styles_report_preview_markdown_sections_and_level_counts() {
        let heading = styled_preview_line("## Level counts".to_string());
        let count = styled_preview_line("- WARN: 2".to_string());

        assert_eq!(heading.spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(count.spans[1].style.fg, Some(Color::Yellow));
        assert_eq!(count.spans[3].style.fg, Some(Color::LightGreen));
    }

    #[test]
    fn renders_footer_operation_hint() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();

        terminal.draw(|frame| render_app(frame, &app)).unwrap();

        let output = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(output.contains("/ search"));
    }
}
