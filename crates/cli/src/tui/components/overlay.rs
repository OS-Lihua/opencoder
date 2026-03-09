//! Generic overlay rendering utilities (centered dialog boxes).

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear};

use crate::tui::theme;

/// Render a centered box overlay on top of the current screen.
///
/// Clears the background area and draws a bordered box with the given title.
/// Returns the inner area for the caller to render content into.
pub fn render_centered_box(f: &mut Frame, title: &str, width_pct: u16, height_pct: u16) -> Rect {
    let area = f.area();

    let width = (area.width as u32 * width_pct as u32 / 100) as u16;
    let height = (area.height as u32 * height_pct as u32 / 100) as u16;

    // Clamp to minimum useful size
    let width = width.max(40).min(area.width);
    let height = height.max(10).min(area.height);

    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;

    let dialog_area = Rect::new(x, y, width, height);

    // Clear the background
    f.render_widget(Clear, dialog_area);

    // Render the border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::title_style())
        .title(format!(" {title} "))
        .title_style(theme::title_style());

    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);

    inner
}
