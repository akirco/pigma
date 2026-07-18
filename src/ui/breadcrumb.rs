use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{create_block, styled_text};
use crate::state::NavState;
use crate::theme::Theme;

pub fn render_breadcrumb(
    f: &mut Frame,
    nav: &NavState,
    colors: &Theme,
    bordered: bool,
    border_rounded: bool,
    area: Rect,
) {
    let (section, item) = if nav.sections.is_empty() {
        ("", "")
    } else {
        let s = &nav.sections[nav.focus_section];
        let name = nav.section_states[nav.focus_section]
            .selected()
            .and_then(|i| s.items.get(i).map(|it| it.name.as_str()))
            .unwrap_or("");
        (s.title.as_str(), name)
    };

    let line = if let Some(sub) = &nav.subtitle {
        let mut parts = styled_text::parse_styled(section, colors);
        if !item.is_empty() {
            parts.push(Span::styled(" / ", Style::default().fg(colors.muted)));
            parts.push(Span::styled(
                item,
                Style::default()
                    .fg(colors.muted)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        parts.push(Span::styled(" / ", Style::default().fg(colors.muted)));
        parts.push(Span::styled(
            sub,
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ));
        Line::from(parts)
    } else if item.is_empty() {
        Line::from(styled_text::parse_styled(section, colors))
    } else {
        let mut parts = styled_text::parse_styled(section, colors);
        parts.push(Span::styled(" / ", Style::default().fg(colors.muted)));
        parts.push(Span::styled(
            item,
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ));
        Line::from(parts)
    };

    let block = create_block("", colors, bordered, border_rounded, false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(line), inner);
}
