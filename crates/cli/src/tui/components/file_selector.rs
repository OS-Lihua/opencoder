//! File selector overlay: fuzzy file search triggered by @.

use std::path::Path;

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::tui::theme;

/// State for the file selector overlay.
pub struct FileSelectorState {
    pub query: String,
    pub all_files: Vec<String>,
    pub matches: Vec<String>,
    pub selected: usize,
}

impl FileSelectorState {
    pub fn new(project_dir: &Path) -> Self {
        let all_files = enumerate_files(project_dir);
        Self {
            query: String::new(),
            all_files,
            matches: Vec::new(),
            selected: 0,
        }
    }

    pub fn update_matches(&mut self) {
        if self.query.is_empty() {
            self.matches = self.all_files.iter().take(20).cloned().collect();
        } else {
            let q = self.query.to_lowercase();
            let mut scored: Vec<(f64, &String)> = self
                .all_files
                .iter()
                .filter_map(|f| {
                    let fl = f.to_lowercase();
                    if fl.contains(&q) {
                        Some((1.0, f))
                    } else {
                        let score = strsim::jaro_winkler(&q, &fl);
                        if score > 0.6 { Some((score, f)) } else { None }
                    }
                })
                .collect();
            // Exact substring matches first, then by score
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            self.matches = scored
                .into_iter()
                .take(20)
                .map(|(_, f)| f.clone())
                .collect();
        }
        if self.selected >= self.matches.len() {
            self.selected = 0;
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
        self.update_matches();
    }

    pub fn delete_char(&mut self) {
        self.query.pop();
        self.update_matches();
    }

    #[allow(dead_code)]
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    #[allow(dead_code)]
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.matches.len() {
            self.selected += 1;
        }
    }

    pub fn selected_file(&self) -> Option<&str> {
        self.matches.get(self.selected).map(|s| s.as_str())
    }
}

/// Enumerate project files using the `ignore` crate (respects .gitignore).
fn enumerate_files(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let walker = ignore::WalkBuilder::new(dir)
        .hidden(true)
        .max_depth(Some(8))
        .build();
    for entry in walker.flatten() {
        if entry.file_type().is_some_and(|ft| ft.is_file())
            && let Ok(rel) = entry.path().strip_prefix(dir)
        {
            files.push(rel.to_string_lossy().to_string());
        }
    }
    files.sort();
    files
}

/// Render the file selector overlay.
pub fn render(f: &mut Frame, state: &FileSelectorState) {
    let area = f.area();
    let width = (area.width / 2).max(40).min(area.width.saturating_sub(4));
    let height = 15u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup = Rect::new(x, y, width, height);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .title(format!(" @ File: {} ", state.query))
        .borders(Borders::ALL)
        .style(theme::border_style());

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let items: Vec<ListItem> = state
        .matches
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let style = if i == state.selected {
                theme::selected_style()
            } else {
                theme::normal_style()
            };
            ListItem::new(Span::styled(file.clone(), style))
        })
        .collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}
