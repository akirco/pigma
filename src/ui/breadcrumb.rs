use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::BlockStyle;
use super::{create_block, styled_text};
use crate::state::NavState;

pub fn render_breadcrumb(f: &mut Frame, nav: &NavState, bs: &BlockStyle<'_>, area: Rect) {
    let colors = bs.colors;

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

    let muted = Style::default().fg(colors.muted);
    let text_style = Style::default().fg(colors.text);

    let line = if let Some(sub) = &nav.subtitle {
        let mut line = Line::from(styled_text::parse_styled_with(section, colors, muted));
        if !item.is_empty() {
            line.push_span(Span::styled(" / ", muted));
            for s in styled_text::parse_styled_with(item, colors, muted) {
                line.push_span(s);
            }
        }
        line.push_span(Span::styled(" / ", muted));
        for s in styled_text::parse_styled_with(sub, colors, text_style) {
            line.push_span(s);
        }
        line
    } else if item.is_empty() {
        Line::from(styled_text::parse_styled_with(section, colors, text_style))
    } else {
        let mut line = Line::from(styled_text::parse_styled_with(section, colors, muted));
        line.push_span(Span::styled(" / ", muted));
        for s in styled_text::parse_styled_with(item, colors, text_style) {
            line.push_span(s);
        }
        line
    };

    let block = create_block("", bs, false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(line), inner);
}
