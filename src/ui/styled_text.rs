use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::config::Theme;

/// Parse `<tag>text</tag>` markup into styled `Vec<Span>`.
///
/// Supported tags:
/// - Theme colors: `<accent>`, `<text>`, `<muted>`, `<error>`, `<warning>`, `<highlight>`, `<bg>`, `<surface>`
/// - Modifiers: `<b>` (bold), `<i>` (italic), `<dim>` (dimmed)
/// - Literal colors: `<#rrggbb>`, or any name accepted by `ratatui::style::Color::from_str`
///
/// Text without tags is rendered as plain spans with no styling.
pub fn parse_styled<'a>(text: &'a str, theme: &Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let mut tag_stack: Vec<Style> = Vec::new();
    let mut current_style = Style::default();
    let mut pos = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();

    while pos < len {
        if bytes[pos] == b'<' {
            let tag_start = pos + 1;
            let mut tag_end = tag_start;
            while tag_end < len && bytes[tag_end] != b'>' {
                tag_end += 1;
            }
            if tag_end >= len {
                spans.push(Span::styled(&text[pos..pos + 1], current_style));
                pos += 1;
                continue;
            }

            let tag_content = &text[tag_start..tag_end];
            pos = tag_end + 1;

            if tag_content.starts_with('/') {
                tag_stack.pop().inspect(|s| current_style = *s);
            } else {
                tag_stack.push(current_style);
                current_style = apply_tag(tag_content, current_style, theme);
            }
        } else {
            let start = pos;
            while pos < len && bytes[pos] != b'<' {
                pos += 1;
            }
            let slice = &text[start..pos];
            if !slice.is_empty() {
                spans.push(Span::styled(slice, current_style));
            }
        }
    }

    spans
}

fn apply_tag(tag: &str, current: Style, theme: &Theme) -> Style {
    match tag {
        "b" => current.add_modifier(Modifier::BOLD),
        "i" => current.add_modifier(Modifier::ITALIC),
        "dim" => current.add_modifier(Modifier::DIM),
        _ => {
            let is_theme_color = matches!(
                tag,
                "bg" | "surface" | "text" | "accent" | "highlight" | "muted" | "error" | "warning"
            );
            if is_theme_color {
                current.fg(theme.field_color(tag))
            } else if let Ok(c) = tag.parse::<Color>() {
                current.fg(c)
            } else {
                current
            }
        }
    }
}
