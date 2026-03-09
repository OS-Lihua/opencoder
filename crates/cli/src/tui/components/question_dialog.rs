//! Question dialog overlay component.

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::tui::app::QuestionDialogState;
use crate::tui::theme;

use super::overlay;

pub fn render(f: &mut Frame, state: &QuestionDialogState) {
    let inner = overlay::render_centered_box(f, "Question", 60, 40);

    if inner.height < 5 || inner.width < 10 {
        return;
    }

    if state.options.is_empty() {
        // Free-text input mode
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(2),   // question text
                Constraint::Length(1), // blank
                Constraint::Length(3), // input area
                Constraint::Length(1), // hint
            ])
            .split(inner);

        // Question text
        f.render_widget(
            Paragraph::new(state.question_text.as_str())
                .style(theme::normal_style())
                .wrap(ratatui::widgets::Wrap { trim: false }),
            chunks[0],
        );

        // Input area
        let input_text = format!(">{} ", state.input);
        f.render_widget(
            Paragraph::new(input_text).style(theme::input_style()),
            chunks[2],
        );

        // Place cursor after input
        let cursor_x = chunks[2].x + 2 + state.input.len() as u16;
        let cursor_y = chunks[2].y;
        if cursor_x < chunks[2].right() {
            f.set_cursor_position((cursor_x, cursor_y));
        }

        // Hint
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "Enter submit  Esc cancel",
                theme::dim_style(),
            )))
            .alignment(Alignment::Center),
            chunks[3],
        );
    } else {
        // Option selection mode
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(2),   // question text
                Constraint::Length(1), // blank
                Constraint::Min(1),   // options list
                Constraint::Length(1), // hint
            ])
            .split(inner);

        // Question text
        f.render_widget(
            Paragraph::new(state.question_text.as_str())
                .style(theme::normal_style())
                .wrap(ratatui::widgets::Wrap { trim: false }),
            chunks[0],
        );

        // Options
        let mut lines = Vec::new();
        for (i, opt) in state.options.iter().enumerate() {
            let style = if i == state.selected_option {
                theme::selected_style().add_modifier(Modifier::BOLD)
            } else {
                theme::normal_style()
            };
            let marker = if i == state.selected_option {
                "▸ "
            } else {
                "  "
            };
            lines.push(Line::from(Span::styled(format!("{marker}{opt}"), style)));
        }
        f.render_widget(Paragraph::new(lines), chunks[2]);

        // Hint
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "↑↓ select  Enter confirm  Esc cancel",
                theme::dim_style(),
            )))
            .alignment(Alignment::Center),
            chunks[3],
        );
    }
}
