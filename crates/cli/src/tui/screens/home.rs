//! Home screen: session list.

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::tui::app::App;
use crate::tui::theme;

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Min(5),    // session list
            Constraint::Length(3), // status bar
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new(format!(" opencoder v{} ", env!("CARGO_PKG_VERSION")))
        .style(theme::title_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(theme::border_style()),
        );
    f.render_widget(title, chunks[0]);

    // Session list
    let items: Vec<ListItem> = app
        .sessions
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let ts = chrono::DateTime::from_timestamp_millis(session.time_created)
                .map(|dt| dt.format("%m/%d %H:%M").to_string())
                .unwrap_or_default();
            let content = format!("  {}  {}", session.title, ts);
            let style = if i == app.selected_session {
                theme::selected_style()
            } else {
                theme::normal_style()
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Sessions ")
                .borders(Borders::ALL)
                .style(theme::border_style()),
        )
        .highlight_style(theme::selected_style());

    if app.sessions.is_empty() {
        let empty = Paragraph::new("  No sessions. Press 'n' to create one.")
            .style(theme::dim_style())
            .block(
                Block::default()
                    .title(" Sessions ")
                    .borders(Borders::ALL)
                    .style(theme::border_style()),
            );
        f.render_widget(empty, chunks[1]);
    } else {
        f.render_widget(list, chunks[1]);
    }

    // Status bar
    let status = Paragraph::new(" n=New  d=Delete  Enter=Open  /=Search  q=Quit")
        .style(theme::dim_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(theme::border_style()),
        );
    f.render_widget(status, chunks[2]);
}
