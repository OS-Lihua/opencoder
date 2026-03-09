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
            match &tool.state {
                ToolState::Pending { .. } => {
                    lines.push(Line::from(vec![
                        Span::styled("  ◌ ", theme::tool_pending_style()),
                        Span::styled(tool.tool.clone(), theme::tool_pending_style()),
                        Span::styled(": ", theme::dim_style()),
                        Span::styled("pending...", theme::tool_pending_style()),
                    ]));
                }
                ToolState::Running { title, .. } => {
                    let t = title.as_deref().unwrap_or("running...");
                    lines.push(Line::from(vec![
                        Span::styled("  ⟳ ", theme::tool_running_style()),
                        Span::styled(tool.tool.clone(), theme::tool_running_style()),
                        Span::styled(": ", theme::dim_style()),
                        Span::styled(t.to_string(), theme::tool_running_style()),
                    ]));
                }
                ToolState::Completed {
                    title,
                    output,
                    time_start,
                    time_end,
                    ..
                } => {
                    let duration = format_duration(*time_end - *time_start);
                    lines.push(Line::from(vec![
                        Span::styled("  ✓ ", theme::tool_completed_style()),
                        Span::styled(tool.tool.clone(), theme::tool_completed_style()),
                        Span::styled(format!(" ({duration})"), theme::dim_style()),
                        Span::styled(": ", theme::dim_style()),
                        Span::styled(title.clone(), theme::tool_completed_style()),
                    ]));
                    // Show output preview (up to 3 lines)
                    let preview = truncate_output(output, 3);
                    if !preview.is_empty() {
                        for line in preview.lines() {
                            lines.push(Line::from(Span::styled(
                                format!("    {line}"),
                                theme::dim_style(),
                            )));
                        }
                    }
                }
                ToolState::Error { error, .. } => {
                    lines.push(Line::from(vec![
                        Span::styled("  ✗ ", theme::tool_error_style()),
                        Span::styled(tool.tool.clone(), theme::tool_error_style()),
                        Span::styled(": ", theme::dim_style()),
                        Span::styled(error.clone(), theme::tool_error_style()),
                    ]));
                }
            }
        }
        Part::Reasoning(reasoning) => {
            if reasoning.content.is_empty() {
                return;
            }
            let words = reasoning.content.split_whitespace().count();
            lines.push(Line::from(Span::styled(
                format!("  [thinking... {words} words]"),
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

/// Truncate output to the first N lines, showing a "... (X more lines)" indicator.
fn truncate_output(output: &str, max_lines: usize) -> String {
    let output = output.trim();
    if output.is_empty() {
        return String::new();
    }
    let all_lines: Vec<&str> = output.lines().collect();
    if all_lines.len() <= max_lines {
        output.to_string()
    } else {
        let shown: Vec<&str> = all_lines[..max_lines].to_vec();
        let remaining = all_lines.len() - max_lines;
        format!("{}\n... ({remaining} more lines)", shown.join("\n"))
    }
}

/// Format a duration in milliseconds as a human-readable string.
fn format_duration(ms: i64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        let secs = ms as f64 / 1000.0;
        format!("{secs:.1}s")
    }
}
