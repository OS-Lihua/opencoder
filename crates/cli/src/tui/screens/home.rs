//! Home screen: session list.

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::tui::app::App;
use crate::tui::theme;

pub fn render(f: &mut Frame, app: &App) {
    let constraints = if app.searching {
        vec![
            Constraint::Length(3), // title
            Constraint::Length(3), // search box
            Constraint::Min(5),    // session list
            Constraint::Length(3), // status bar
        ]
    } else {
        vec![
            Constraint::Length(3), // title
            Constraint::Min(5),    // session list
            Constraint::Length(3), // status bar
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
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

    let (list_chunk, status_chunk) = if app.searching {
        // Search box
        let search_input = Paragraph::new(format!(" {}", app.search_query))
            .style(theme::input_style())
            .block(
                Block::default()
                    .title(" Search (Esc=close) ")
                    .borders(Borders::ALL)
                    .style(theme::border_style()),
            );
        f.render_widget(search_input, chunks[1]);

        // Set cursor in search box
        let cursor_x = chunks[1].x + app.search_query.len() as u16 + 2;
        let cursor_y = chunks[1].y + 1;
        f.set_cursor_position((cursor_x.min(chunks[1].right() - 2), cursor_y));

        (chunks[2], chunks[3])
    } else {
        (chunks[1], chunks[2])
    };

    // Filter sessions by search query
    let filtered_sessions: Vec<(usize, &opencoder_session::Session)> = app
        .sessions
        .iter()
        .enumerate()
        .filter(|(_, session)| {
            if app.search_query.is_empty() {
                return true;
            }
            let query = app.search_query.to_lowercase();
            session.title.to_lowercase().contains(&query)
        })
        .collect();

    // Session list
    let items: Vec<ListItem> = filtered_sessions
        .iter()
        .map(|(i, session)| {
            let ts = chrono::DateTime::from_timestamp_millis(session.time_created)
                .map(|dt| dt.format("%m/%d %H:%M").to_string())
                .unwrap_or_default();
            let content = format!("  {}  {}", session.title, ts);
            let style = if *i == app.selected_session {
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

    if filtered_sessions.is_empty() {
        let msg = if app.search_query.is_empty() {
            "  No sessions. Press 'n' to create one."
        } else {
            "  No matching sessions."
        };
        let empty = Paragraph::new(msg).style(theme::dim_style()).block(
            Block::default()
                .title(" Sessions ")
                .borders(Borders::ALL)
                .style(theme::border_style()),
        );
        f.render_widget(empty, list_chunk);
    } else {
        f.render_widget(list, list_chunk);
    }

    // Status bar
    let status = Paragraph::new(" n=New  d=Delete  Enter=Open  /=Search  q=Quit")
        .style(theme::dim_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(theme::border_style()),
        );
    f.render_widget(status, status_chunk);
}
