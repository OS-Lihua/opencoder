//! Markdown to ratatui Lines renderer with syntax highlighting.

use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use crate::tui::theme;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static HIGHLIGHT_THEME: LazyLock<syntect::highlighting::Theme> =
    LazyLock::new(|| ThemeSet::load_defaults().themes["base16-ocean.dark"].clone());

/// Render markdown text into ratatui Lines.
pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let parser = Parser::new(text);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = vec![theme::assistant_style()];
    let mut in_code_block = false;
    let mut code_block_lang: String = String::new();
    let mut code_block_content: String = String::new();
    let mut list_depth: usize = 0;
    let mut ordered_index: Option<u64> = None;
    let mut in_heading = false;

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { .. } => {
                    flush_line(&mut current_spans, &mut lines);
                    style_stack.push(theme::heading_style());
                    in_heading = true;
                }
                Tag::Strong => {
                    let current = current_style(&style_stack);
                    style_stack.push(current.add_modifier(ratatui::style::Modifier::BOLD));
                }
                Tag::Emphasis => {
                    let current = current_style(&style_stack);
                    style_stack.push(current.add_modifier(ratatui::style::Modifier::ITALIC));
                }
                Tag::CodeBlock(kind) => {
                    flush_line(&mut current_spans, &mut lines);
                    in_code_block = true;
                    code_block_content.clear();
                    code_block_lang = match &kind {
                        CodeBlockKind::Fenced(lang) => lang.as_ref().to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    if !code_block_lang.is_empty() {
                        lines.push(Line::from(Span::styled(
                            format!("  [{code_block_lang}]"),
                            theme::dim_style(),
                        )));
                    }
                }
                Tag::List(start) => {
                    flush_line(&mut current_spans, &mut lines);
                    list_depth += 1;
                    ordered_index = start;
                }
                Tag::Item => {
                    flush_line(&mut current_spans, &mut lines);
                    let indent = "  ".repeat(list_depth);
                    let bullet = if let Some(ref mut idx) = ordered_index {
                        let b = format!("{idx}. ");
                        *idx += 1;
                        b
                    } else {
                        "- ".to_string()
                    };
                    current_spans.push(Span::styled(
                        format!("{indent}{bullet}"),
                        theme::list_bullet_style(),
                    ));
                }
                Tag::Paragraph => {
                    flush_line(&mut current_spans, &mut lines);
                }
                Tag::BlockQuote(_) => {
                    flush_line(&mut current_spans, &mut lines);
                    current_spans.push(Span::styled("  > ", theme::dim_style()));
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(_) => {
                    flush_line(&mut current_spans, &mut lines);
                    style_stack.pop();
                    in_heading = false;
                }
                TagEnd::Strong | TagEnd::Emphasis => {
                    style_stack.pop();
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    highlight_code_block(&code_block_content, &code_block_lang, &mut lines);
                    code_block_content.clear();
                }
                TagEnd::List(_) => {
                    list_depth = list_depth.saturating_sub(1);
                    if list_depth == 0 {
                        ordered_index = None;
                    }
                }
                TagEnd::Item => {
                    flush_line(&mut current_spans, &mut lines);
                }
                TagEnd::Paragraph => {
                    flush_line(&mut current_spans, &mut lines);
                }
                TagEnd::BlockQuote(_) => {
                    flush_line(&mut current_spans, &mut lines);
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    code_block_content.push_str(text.as_ref());
                } else {
                    let style = current_style(&style_stack);
                    current_spans.push(Span::styled(text.to_string(), style));
                }
            }
            Event::Code(code) => {
                current_spans.push(Span::styled(
                    format!("`{code}`"),
                    theme::code_inline_style(),
                ));
            }
            Event::SoftBreak => {
                if !in_heading {
                    flush_line(&mut current_spans, &mut lines);
                } else {
                    current_spans.push(Span::raw(" "));
                }
            }
            Event::HardBreak => {
                flush_line(&mut current_spans, &mut lines);
            }
            Event::Rule => {
                flush_line(&mut current_spans, &mut lines);
                lines.push(Line::from(Span::styled(
                    "────────────────────────────────",
                    theme::dim_style(),
                )));
            }
            _ => {}
        }
    }

    flush_line(&mut current_spans, &mut lines);
    lines
}

/// Highlight a code block using syntect, falling back to plain style.
fn highlight_code_block(code: &str, lang: &str, lines: &mut Vec<Line<'static>>) {
    let syntax = if lang.is_empty() {
        None
    } else {
        SYNTAX_SET.find_syntax_by_token(lang)
    };

    match syntax {
        Some(syntax) => {
            let mut highlighter = HighlightLines::new(syntax, &HIGHLIGHT_THEME);
            for line in code.lines() {
                match highlighter.highlight_line(line, &SYNTAX_SET) {
                    Ok(ranges) => {
                        let mut spans: Vec<Span<'static>> = vec![Span::raw("  ")];
                        for (style, text) in ranges {
                            let fg = Color::Rgb(
                                style.foreground.r,
                                style.foreground.g,
                                style.foreground.b,
                            );
                            spans.push(Span::styled(text.to_string(), Style::default().fg(fg)));
                        }
                        lines.push(Line::from(spans));
                    }
                    Err(_) => {
                        lines.push(Line::from(Span::styled(
                            format!("  {line}"),
                            theme::code_block_style(),
                        )));
                    }
                }
            }
        }
        None => {
            for line in code.lines() {
                lines.push(Line::from(Span::styled(
                    format!("  {line}"),
                    theme::code_block_style(),
                )));
            }
        }
    }
}

fn current_style(stack: &[Style]) -> Style {
    stack.last().copied().unwrap_or(theme::assistant_style())
}

fn flush_line(spans: &mut Vec<Span<'static>>, lines: &mut Vec<Line<'static>>) {
    if !spans.is_empty() {
        lines.push(Line::from(std::mem::take(spans)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_plain_text() {
        let lines = render_markdown("Hello world");
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_heading() {
        let lines = render_markdown("# Title\n\nBody text");
        assert!(lines.len() >= 2);
    }

    #[test]
    fn render_code_block() {
        let lines = render_markdown("```rust\nlet x = 1;\n```");
        assert!(lines.len() >= 2);
    }

    #[test]
    fn render_code_block_highlighted() {
        let lines = render_markdown("```python\ndef hello():\n    print('hi')\n```");
        assert!(lines.len() >= 3);
    }

    #[test]
    fn render_inline_code() {
        let lines = render_markdown("Use `foo()` here");
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_list() {
        let lines = render_markdown("- item 1\n- item 2\n- item 3");
        assert!(lines.len() >= 3);
    }
}
