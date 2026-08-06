use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::config::Theme;
use crate::utils::gradient_color;

/// Parse `<tag>text</tag>` markup into styled `Vec<Span>`.
///
/// Supported tags:
/// - Theme colors: `<accent>`, `<text>`, `<muted>`, `<error>`, `<bg>`, `<surface>`
/// - Modifiers: `<b>` (bold), `<i>` (italic), `<dim>` (dimmed)
/// - Literal colors: `<#rrggbb>`, or any name accepted by `ratatui::style::Color::from_str`
/// - Gradient: `<gradient:preset>text</gradient>` or `<grad:preset>text</grad>` (per-char gradient coloring)
///   Presets: warm, cubehelix, rainbow, turbo, spectral, viridis
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
            } else if let Some(preset) = tag_content
                .strip_prefix("gradient:")
                .or_else(|| tag_content.strip_prefix("grad:"))
            {
                let close_tag = if tag_content.starts_with("gradient:") {
                    "</gradient>"
                } else {
                    "</grad>"
                };
                if let Some(rest) = text[pos..].find(close_tag) {
                    let inner = &text[pos..pos + rest];
                    pos = pos + rest + close_tag.len();

                    let char_count = inner.chars().count();
                    for (i, ch) in inner.chars().enumerate() {
                        let t = if char_count <= 1 {
                            0.0
                        } else {
                            i as f32 / (char_count - 1) as f32
                        };
                        let [r, g, b] = gradient_color(preset, t);
                        let style = current_style.fg(Color::Rgb(r, g, b));
                        let byte_start =
                            inner.char_indices().nth(i).map(|(idx, _)| idx).unwrap_or(0);
                        let char_len = ch.len_utf8();
                        spans.push(Span::styled(
                            &inner[byte_start..byte_start + char_len],
                            style,
                        ));
                    }
                } else {
                    // no closing tag found: skip the entire unclosed gradient
                    pos = len;
                }
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
                "bg" | "surface" | "text" | "accent" | "muted" | "error" | "border"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_tag_generates_per_char_spans() {
        let theme = Theme::default();
        // turbo: t=0 is dark, t=0.5 is greenish, t=1 is red — all different
        let spans = parse_styled("<gradient:turbo>ABC</gradient>", &theme);
        assert_eq!(spans.len(), 3);
        let c0 = spans[0].style.fg.unwrap();
        let c1 = spans[1].style.fg.unwrap();
        let c2 = spans[2].style.fg.unwrap();
        assert_ne!(c0, c1);
        assert_ne!(c1, c2);
    }

    #[test]
    fn gradient_tag_single_char() {
        let theme = Theme::default();
        let spans = parse_styled("<grad:warm>X</grad>", &theme);
        assert_eq!(spans.len(), 1);
        assert!(matches!(spans[0].style.fg, Some(Color::Rgb(_, _, _))));
    }

    #[test]
    fn mixed_tags() {
        let theme = Theme::default();
        let spans = parse_styled("hello <gradient:turbo>world</gradient>!", &theme);
        // "hello " = 1 span, "world" = 5 per-char gradient spans, "!" = 1 span
        assert_eq!(spans.len(), 7);
        assert_eq!(spans[0].content, "hello ");
        assert_eq!(spans[6].content, "!");
    }

    #[test]
    fn no_closing_tag_renders_nothing() {
        let theme = Theme::default();
        let spans = parse_styled("prefix <gradient:turbo>unclosed", &theme);
        // only "prefix " is rendered, unclosed gradient is skipped
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "prefix ");
    }

    #[test]
    fn nested_with_gradient() {
        let theme = Theme::default();
        let spans = parse_styled("<b><gradient:warm>hi</gradient></b>", &theme);
        assert_eq!(spans.len(), 2);
        // gradient chars should inherit bold from parent tag
        for s in &spans {
            assert!(s.style.add_modifier.contains(Modifier::BOLD));
        }
    }
}
