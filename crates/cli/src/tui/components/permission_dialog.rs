//! Permission dialog overlay component.

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::tui::app::PermissionDialogState;
use crate::tui::theme;

use super::overlay;

const OPTIONS: [&str; 3] = ["Allow", "Deny", "Always Allow"];

pub fn render(f: &mut Frame, state: &PermissionDialogState) {
    let inner = overlay::render_centered_box(f, "Permission Required", 60, 35);

    if inner.height < 6 || inner.width < 10 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tool name
            Constraint::Length(1), // blank
            Constraint::Min(2),    // description
            Constraint::Length(1), // blank
            Constraint::Length(3), // options
            Constraint::Length(1), // hint
        ])
        .split(inner);

    // Tool name
    let tool_line = Line::from(vec![
        Span::styled("Tool: ", theme::dim_style()),
        Span::styled(&state.tool_name, theme::title_style()),
    ]);
    f.render_widget(Paragraph::new(tool_line), chunks[0]);

    // Description (command / file path)
    let desc_text = if state.description.len() > (inner.width as usize * 3) {
        format!("{}...", &state.description[..inner.width as usize * 3 - 3])
    } else {
        state.description.clone()
    };
    f.render_widget(
        Paragraph::new(desc_text)
            .style(theme::normal_style())
            .wrap(ratatui::widgets::Wrap { trim: false }),
        chunks[2],
    );

    // Options
    let mut option_spans = Vec::new();
    for (i, opt) in OPTIONS.iter().enumerate() {
        if i > 0 {
            option_spans.push(Span::raw("  "));
        }
        if i == state.selected {
            option_spans.push(Span::styled(
                format!("[{opt}]"),
                theme::selected_style().add_modifier(Modifier::BOLD),
            ));
        } else {
            option_spans.push(Span::styled(format!(" {opt} "), theme::dim_style()));
        }
    }
    f.render_widget(
        Paragraph::new(Line::from(option_spans)).alignment(Alignment::Center),
        chunks[4],
    );

    // Hint
    let hint = Line::from(vec![Span::styled(
        "↑↓ select  Enter confirm  Esc deny  y allow  n deny",
        theme::dim_style(),
    )]);
    f.render_widget(Paragraph::new(hint).alignment(Alignment::Center), chunks[5]);
}
