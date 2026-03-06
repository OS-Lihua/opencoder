//! Session screen: conversation view.

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::tui::app::App;
use crate::tui::theme;
use crate::tui::components::message_list;

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // title bar
            Constraint::Min(5),    // messages
            Constraint::Length(5), // input
            Constraint::Length(1), // status
        ])
        .split(f.area());

    // Title bar
    let session_title = app
        .current_session
        .as_ref()
        .map(|s| s.title.as_str())
        .unwrap_or("Session");
    let title = Paragraph::new(format!(" {} ", session_title))
        .style(theme::title_style());
    f.render_widget(title, chunks[0]);

    // Messages area
    let message_lines = message_list::render_messages(&app.messages);
    let total_lines = message_lines.len() as u16;
    let visible_height = chunks[1].height.saturating_sub(2);

    // Auto-scroll to bottom when agent is running
    let scroll = if app.agent_running || app.scroll_offset == 0 {
        total_lines.saturating_sub(visible_height) as u16
    } else {
        (total_lines.saturating_sub(visible_height)).saturating_sub(app.scroll_offset as u16)
    };

    let messages_widget = Paragraph::new(message_lines)
        .scroll((scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(theme::border_style()),
        );
    f.render_widget(messages_widget, chunks[1]);

    // Input area
    let input_block = Block::default()
        .title(if app.agent_running { " Thinking... " } else { " Input (Enter=send, Esc=back) " })
        .borders(Borders::ALL)
        .style(if app.agent_running {
            theme::tool_running_style()
        } else {
            theme::border_style()
        });

    let input = Paragraph::new(app.input.as_str())
        .style(theme::input_style())
        .block(input_block);
    f.render_widget(input, chunks[2]);

    // Set cursor position in input area
    if !app.agent_running {
        let input_x = chunks[2].x + app.input.len() as u16 + 1;
        let input_y = chunks[2].y + 1;
        f.set_cursor_position((input_x.min(chunks[2].right() - 2), input_y));
    }

    // Status bar
    let status_text = if app.agent_running {
        format!(" {} ", app.status_text)
    } else {
        " Ready | Ctrl+C=Cancel | Esc=Home ".to_string()
    };
    let status = Paragraph::new(status_text).style(theme::dim_style());
    f.render_widget(status, chunks[3]);
}
