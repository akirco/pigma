use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};

use super::create_block;
use crate::state::{NavItem, NavState};
use crate::theme::Theme;

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

    let constraints: Vec<Constraint> = nav
        .sections
        .iter()
        .flat_map(|s| {
            vec![
                Constraint::Length(1),
                Constraint::Length(s.items.len() as u16),
            ]
        })
        .collect();

    if constraints.is_empty() {
        return;
    }

    let chunks = Layout::vertical(constraints).split(inner);

    for (i, section) in nav.sections.iter().enumerate() {
        let title_area = chunks[i * 2];
        let items_area = chunks[i * 2 + 1];
        let focused = i == nav.focus_section;

        render_title(f, &section.title, colors, title_area);
        render_items(
            f,
            &section.items,
            &mut nav.section_states[i],
            colors,
            focused,
            items_area,
        );
    }
}

fn render_title(f: &mut Frame, title: &str, colors: &Theme, area: Rect) {
    let line = Line::from(vec![
        Span::styled("▎", Style::default().fg(colors.accent)),
        Span::raw(" "),
        Span::styled(
            title,
            Style::default()
                .fg(colors.text)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_items(
    f: &mut Frame,
    items: &[NavItem],
    state: &mut ratatui::widgets::ListState,
    colors: &Theme,
    focused: bool,
    area: Rect,
) {
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let is_selected = state.selected() == Some(idx);
            let prefix = if is_selected && focused { "▶ " } else { "  " };
            let text = format!("{}{}", prefix, item.name);
            let item_style = if is_selected && focused {
                Style::default()
                    .fg(colors.accent)
                    .bg(colors.surface)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors.text)
            };
            ListItem::new(Line::from(Span::raw(text))).style(item_style)
        })
        .collect();

    let list = List::new(list_items);
    f.render_stateful_widget(list, area, state);
}
