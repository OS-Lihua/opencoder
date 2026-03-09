//! Session screen: conversation view.

use ratatui::prelude::*;
use ratatui::widgets::*;

use opencoder_session::message::Part;

use crate::tui::app::App;
use crate::tui::components::message_list;
use crate::tui::theme;

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
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
    let title = Paragraph::new(format!(" {} ", session_title)).style(theme::title_style());
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

    let messages_widget = Paragraph::new(message_lines).scroll((scroll, 0)).block(
        Block::default()
            .borders(Borders::ALL)
            .style(theme::border_style()),
    );
    f.render_widget(messages_widget, chunks[1]);

    // Input area
    let input_block = Block::default()
        .title(if app.agent_running {
            " Thinking... "
        } else {
            " Input (Enter=send, Esc=back) "
        })
        .borders(Borders::ALL)
        .style(if app.agent_running {
            theme::tool_running_style()
        } else {
            theme::border_style()
        });

    let input = Paragraph::new(app.input_state.text.as_str())
        .style(theme::input_style())
        .block(input_block);
    f.render_widget(input, chunks[2]);

    // Set cursor position in input area
    if !app.agent_running {
        // Calculate display width up to cursor byte position
        let display_cursor = app.input_state.text[..app.input_state.cursor]
            .chars()
            .count() as u16;
        let input_x = chunks[2].x + display_cursor + 1;
        let input_y = chunks[2].y + 1;
        f.set_cursor_position((input_x.min(chunks[2].right() - 2), input_y));
    }

    // Status bar with token usage
    let token_info = compute_token_usage(&app.messages);
    let status_text = if app.agent_running {
        if token_info.is_empty() {
            format!(" {} ", app.status_text)
        } else {
            format!(" {} | {} ", app.status_text, token_info)
        }
    } else {
        let agent_part = format!("[{}]", app.current_agent);
        if token_info.is_empty() {
            format!(" {agent_part} Ready | Ctrl+G=Agent | Ctrl+C=Cancel | Esc=Home ")
        } else {
            format!(" {agent_part} Ready | {token_info} | Ctrl+G=Agent | Ctrl+C=Cancel | Esc=Home ")
        }
    };
    let status = Paragraph::new(status_text).style(theme::dim_style());
    f.render_widget(status, chunks[3]);
}

/// Compute total token usage from StepFinish parts.
fn compute_token_usage(messages: &[opencoder_session::MessageWithParts]) -> String {
    let mut total_input: u64 = 0;
    let mut total_output: u64 = 0;

    for msg in messages {
        for p in &msg.parts {
            if let Part::StepFinish(sf) = &p.part {
                total_input += sf.usage.input_tokens;
                total_output += sf.usage.output_tokens;
            }
        }
    }

    if total_input == 0 && total_output == 0 {
        return String::new();
    }

    format!(
        "{} in / {} out",
        format_tokens(total_input),
        format_tokens(total_output)
    )
}

/// Format a token count as a human-readable string (e.g., "1.2k", "15.3k").
fn format_tokens(n: u64) -> String {
    if n < 1000 {
        format!("{n}")
    } else if n < 100_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        format!("{:.0}k", n as f64 / 1000.0)
    }
}
