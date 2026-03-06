//! Theme colors for the TUI.

use ratatui::style::{Color, Modifier, Style};

pub fn title_style() -> Style {
    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
}

pub fn selected_style() -> Style {
    Style::default().bg(Color::DarkGray).fg(Color::White)
}

pub fn normal_style() -> Style {
    Style::default().fg(Color::White)
}

pub fn dim_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn user_style() -> Style {
    Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
}

pub fn assistant_style() -> Style {
    Style::default().fg(Color::White)
}

pub fn tool_pending_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn tool_running_style() -> Style {
    Style::default().fg(Color::Yellow)
}

pub fn tool_completed_style() -> Style {
    Style::default().fg(Color::Green)
}

pub fn tool_error_style() -> Style {
    Style::default().fg(Color::Red)
}

pub fn status_style() -> Style {
    Style::default().fg(Color::Yellow)
}

pub fn error_style() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}

pub fn input_style() -> Style {
    Style::default().fg(Color::White)
}

pub fn border_style() -> Style {
    Style::default().fg(Color::DarkGray)
}
