//! Message list rendering component.

use ratatui::text::{Line, Span};

use opencoder_session::MessageWithParts;
use opencoder_session::message::*;

use crate::tui::theme;

/// Render messages into Lines for display in a Paragraph.
pub fn render_messages(messages: &[MessageWithParts]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for msg in messages {
        match &msg.message {
            Message::User(user) => {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("> ", theme::user_style()),
                    Span::styled(user.content.clone(), theme::user_style()),
                ]));
            }
            Message::Assistant(_) => {
                for part_with_id in &msg.parts {
                    render_part(&part_with_id.part, &mut lines);
                }
            }
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Start a conversation by typing a message.",
            theme::dim_style(),
        )));
    }

    lines
}

fn render_part(part: &Part, lines: &mut Vec<Line<'static>>) {
    match part {
        Part::Text(text) => {
            if text.content.is_empty() {
                return;
            }
            lines.push(Line::from(""));
            for line in text.content.lines() {
                lines.push(Line::from(Span::styled(
                    line.to_string(),
                    theme::assistant_style(),
                )));
            }
        }
        Part::Tool(tool) => {
            let (icon, style, detail) = match &tool.state {
                ToolState::Pending { .. } => {
                    ("◌", theme::tool_pending_style(), "pending...".to_string())
                }
                ToolState::Running { title, .. } => {
                    let t = title.as_deref().unwrap_or("running...");
                    ("⟳", theme::tool_running_style(), t.to_string())
                }
                ToolState::Completed { title, .. } => {
                    ("✓", theme::tool_completed_style(), title.clone())
                }
                ToolState::Error { error, .. } => {
                    let short = if error.len() > 80 {
                        format!("{}...", &error[..77])
                    } else {
                        error.clone()
                    };
                    ("✗", theme::tool_error_style(), short)
                }
            };

            lines.push(Line::from(vec![
                Span::styled(format!("  {icon} "), style),
                Span::styled(tool.tool.clone(), style),
                Span::styled(": ", theme::dim_style()),
                Span::styled(detail, style),
            ]));
        }
        Part::Reasoning(reasoning) => {
            if reasoning.content.is_empty() {
                return;
            }
            lines.push(Line::from(Span::styled(
                "  [thinking...]".to_string(),
                theme::dim_style(),
            )));
        }
        Part::StepStart(_) | Part::StepFinish(_) => {
            // Optionally render step separators
        }
        Part::Compaction(c) => {
            lines.push(Line::from(Span::styled(
                format!("  [compacted {} messages]", c.compacted_count),
                theme::dim_style(),
            )));
        }
        _ => {}
    }
}
