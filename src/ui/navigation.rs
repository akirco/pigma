use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem},
};

use super::{create_block, styled_text};
use crate::config::Theme;
use crate::state::NavState;

pub fn draw(
    f: &mut Frame,
    nav: &mut NavState,
    colors: &Theme,
    bordered: bool,
    border_rounded: bool,
    title: &str,
    area: Rect,
) {
    let block = create_block(title, colors, bordered, border_rounded, false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if nav.sections.is_empty() {
        return;
    }

    let total_rows: usize = nav.sections.iter().map(|s| s.items.len() + 1).sum();
    let mut list_items = Vec::with_capacity(total_rows);

    let mut global_selected_idx = None;
    let mut current_global_row = 0;

    for (i, section) in nav.sections.iter().enumerate() {
        let focused = i == nav.focus_section;

        let title_spans = styled_text::parse_styled(&section.title, colors);
        list_items.push(ListItem::new(Line::from(title_spans)));
        current_global_row += 1;

        let state = &nav.section_states[i];
        for (idx, item) in section.items.iter().enumerate() {
            let is_selected = state.selected() == Some(idx);

            if is_selected && focused {
                global_selected_idx = Some(current_global_row);
            }

            let prefix = if is_selected && focused { "▶ " } else { "  " };
            let item_style = if is_selected && focused {
                Style::default()
                    .fg(colors.accent)
                    .bg(colors.surface)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors.text)
            };

            let name_spans = styled_text::parse_styled(&item.name, colors);
            let mut spans = Vec::with_capacity(name_spans.len() + 1);
            spans.push(Span::styled(prefix, item_style));
            spans.extend(name_spans);

            list_items.push(ListItem::new(Line::from(spans)).style(item_style));
            current_global_row += 1;
        }
    }

    let list = List::new(list_items);
    let mut global_state =
        ratatui::widgets::ListState::default().with_selected(global_selected_idx);

    f.render_stateful_widget(list, inner, &mut global_state);
}
