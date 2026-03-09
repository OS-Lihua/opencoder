//! Theme colors for the TUI.
//!
//! Provides both static convenience functions (using a global default theme)
//! and a `Theme` struct that can be loaded from config.

use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

/// A configurable theme with all style definitions.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub title: ThemeStyle,
    pub selected: ThemeStyle,
    pub normal: ThemeStyle,
    pub dim: ThemeStyle,
    pub user: ThemeStyle,
    pub assistant: ThemeStyle,
    pub tool_pending: ThemeStyle,
    pub tool_running: ThemeStyle,
    pub tool_completed: ThemeStyle,
    pub tool_error: ThemeStyle,
    pub heading: ThemeStyle,
    pub code_inline: ThemeStyle,
    pub code_block: ThemeStyle,
    pub list_bullet: ThemeStyle,
    pub input: ThemeStyle,
    pub border: ThemeStyle,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            title: ThemeStyle {
                fg: Some("cyan".into()),
                bg: None,
                bold: true,
                italic: false,
            },
            selected: ThemeStyle {
                fg: Some("white".into()),
                bg: Some("darkgray".into()),
                bold: false,
                italic: false,
            },
            normal: ThemeStyle {
                fg: Some("white".into()),
                bg: None,
                bold: false,
                italic: false,
            },
            dim: ThemeStyle {
                fg: Some("darkgray".into()),
                bg: None,
                bold: false,
                italic: false,
            },
            user: ThemeStyle {
                fg: Some("blue".into()),
                bg: None,
                bold: true,
                italic: false,
            },
            assistant: ThemeStyle {
                fg: Some("white".into()),
                bg: None,
                bold: false,
                italic: false,
            },
            tool_pending: ThemeStyle {
                fg: Some("darkgray".into()),
                bg: None,
                bold: false,
                italic: false,
            },
            tool_running: ThemeStyle {
                fg: Some("yellow".into()),
                bg: None,
                bold: false,
                italic: false,
            },
            tool_completed: ThemeStyle {
                fg: Some("green".into()),
                bg: None,
                bold: false,
                italic: false,
            },
            tool_error: ThemeStyle {
                fg: Some("red".into()),
                bg: None,
                bold: false,
                italic: false,
            },
            heading: ThemeStyle {
                fg: Some("cyan".into()),
                bg: None,
                bold: true,
                italic: false,
            },
            code_inline: ThemeStyle {
                fg: Some("yellow".into()),
                bg: None,
                bold: false,
                italic: false,
            },
            code_block: ThemeStyle {
                fg: Some("gray".into()),
                bg: None,
                bold: false,
                italic: false,
            },
            list_bullet: ThemeStyle {
                fg: Some("darkgray".into()),
                bg: None,
                bold: false,
                italic: false,
            },
            input: ThemeStyle {
                fg: Some("white".into()),
                bg: None,
                bold: false,
                italic: false,
            },
            border: ThemeStyle {
                fg: Some("darkgray".into()),
                bg: None,
                bold: false,
                italic: false,
            },
        }
    }
}

#[allow(dead_code)]
impl Theme {
    /// Load theme from a config JSON value, falling back to defaults.
    pub fn from_config(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }
}

/// A single style definition that can be serialized/deserialized.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: bool,
    pub italic: bool,
}

#[allow(dead_code)]
impl ThemeStyle {
    pub fn to_style(&self) -> Style {
        let mut style = Style::default();
        if let Some(ref fg) = self.fg
            && let Some(color) = parse_color(fg)
        {
            style = style.fg(color);
        }
        if let Some(ref bg) = self.bg
            && let Some(color) = parse_color(bg)
        {
            style = style.bg(color);
        }
        if self.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        style
    }
}

#[allow(dead_code)]
fn parse_color(s: &str) -> Option<Color> {
    match s.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" | "dark_gray" | "dark_grey" => Some(Color::DarkGray),
        "lightred" | "light_red" => Some(Color::LightRed),
        "lightgreen" | "light_green" => Some(Color::LightGreen),
        "lightyellow" | "light_yellow" => Some(Color::LightYellow),
        "lightblue" | "light_blue" => Some(Color::LightBlue),
        "lightmagenta" | "light_magenta" => Some(Color::LightMagenta),
        "lightcyan" | "light_cyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        _ => {
            // Try #RRGGBB hex
            if s.starts_with('#') && s.len() == 7 {
                let r = u8::from_str_radix(&s[1..3], 16).ok()?;
                let g = u8::from_str_radix(&s[3..5], 16).ok()?;
                let b = u8::from_str_radix(&s[5..7], 16).ok()?;
                Some(Color::Rgb(r, g, b))
            } else {
                None
            }
        }
    }
}

// === Static convenience functions (default theme) ===

pub fn title_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
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
    Style::default()
        .fg(Color::Blue)
        .add_modifier(Modifier::BOLD)
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

pub fn heading_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

pub fn code_inline_style() -> Style {
    Style::default().fg(Color::Yellow)
}

pub fn code_block_style() -> Style {
    Style::default().fg(Color::Gray)
}

pub fn list_bullet_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn input_style() -> Style {
    Style::default().fg(Color::White)
}

pub fn border_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme() {
        let theme = Theme::default();
        let style = theme.title.to_style();
        assert_eq!(style, title_style());
    }

    #[test]
    fn parse_hex_color() {
        let color = parse_color("#ff0000");
        assert_eq!(color, Some(Color::Rgb(255, 0, 0)));
    }

    #[test]
    fn theme_from_config() {
        let json = serde_json::json!({
            "title": {"fg": "#00ff00", "bold": true}
        });
        let theme = Theme::from_config(&json);
        assert_eq!(theme.title.fg, Some("#00ff00".to_string()));
        assert!(theme.title.bold);
    }
}
