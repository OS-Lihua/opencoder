//! Agent selector overlay component.

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::tui::app::AgentSelectorState;
use crate::tui::theme;

pub fn render(f: &mut Frame, state: &AgentSelectorState) {
    let area = centered_rect(50, 40, f.area());

    // Clear background
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = state
        .agents
        .iter()
        .enumerate()
        .map(|(i, (name, desc))| {
            let style = if i == state.selected {
                theme::selected_style()
            } else {
                theme::normal_style()
            };
            ListItem::new(format!("  {name:<12} {desc}")).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Select Agent (Enter=confirm, Esc=cancel) ")
            .borders(Borders::ALL)
            .style(theme::border_style()),
    );

    f.render_widget(list, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
