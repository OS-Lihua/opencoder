//! Model selector overlay: switch LLM model from within the TUI.

use ratatui::prelude::*;
use ratatui::widgets::*;

use opencoder_provider::models_db::ModelsDb;

use crate::tui::theme;

/// State for the model selector overlay.
pub struct ModelSelectorState {
    pub models: Vec<(String, String)>, // (provider/model, display name)
    pub selected: usize,
    pub query: String,
    pub filtered: Vec<usize>, // indices into models
}

impl ModelSelectorState {
    pub fn new(current_model: &str) -> Self {
        let db = ModelsDb::load();
        let mut models = Vec::new();

        for provider in db.providers() {
            for model in db.list_for_provider(&provider) {
                let full_id = format!("{}/{}", provider, model.id);
                let display = if model.name.is_empty() {
                    full_id.clone()
                } else {
                    format!("{} ({})", model.name, full_id)
                };
                models.push((full_id, display));
            }
        }

        // Sort models alphabetically
        models.sort_by(|a, b| a.0.cmp(&b.0));

        let filtered: Vec<usize> = (0..models.len()).collect();
        let selected = models
            .iter()
            .position(|(id, _)| id == current_model)
            .unwrap_or(0);

        Self {
            models,
            selected,
            query: String::new(),
            filtered,
        }
    }

    pub fn update_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.models.len()).collect();
        } else {
            let q = self.query.to_lowercase();
            self.filtered = self
                .models
                .iter()
                .enumerate()
                .filter(|(_, (id, name))| {
                    id.to_lowercase().contains(&q) || name.to_lowercase().contains(&q)
                })
                .map(|(i, _)| i)
                .collect();
        }
        if self.selected >= self.filtered.len() {
            self.selected = 0;
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
        self.update_filter();
    }

    pub fn delete_char(&mut self) {
        self.query.pop();
        self.update_filter();
    }

    #[allow(dead_code)]
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    #[allow(dead_code)]
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
        }
    }

    pub fn selected_model(&self) -> Option<&str> {
        self.filtered
            .get(self.selected)
            .and_then(|&idx| self.models.get(idx))
            .map(|(id, _)| id.as_str())
    }
}

/// Render the model selector overlay.
pub fn render(f: &mut Frame, state: &ModelSelectorState) {
    let area = f.area();
    let width = (area.width * 2 / 3)
        .max(50)
        .min(area.width.saturating_sub(4));
    let height = 20u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup = Rect::new(x, y, width, height);

    f.render_widget(Clear, popup);

    let title = if state.query.is_empty() {
        " Select Model (type to filter) ".to_string()
    } else {
        format!(" Model: {} ", state.query)
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(theme::border_style());

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let items: Vec<ListItem> = state
        .filtered
        .iter()
        .enumerate()
        .map(|(i, &model_idx)| {
            let (_, display) = &state.models[model_idx];
            let style = if i == state.selected {
                theme::selected_style()
            } else {
                theme::normal_style()
            };
            ListItem::new(Span::styled(display.clone(), style))
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}
