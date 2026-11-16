use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::BlockStyle;
use crate::{
    config::Theme,
    state::NavState,
    ui::{block::StyledBlock, styled_text},
};

/// Map a sidebar list row (counted from the first visible row) to a nav item.
/// Section title rows map to nothing.
fn item_at_row(sections: &[crate::state::NavSection], row: usize) -> Option<(usize, usize)> {
    let mut start = 0;
    for (section_index, section) in sections.iter().enumerate() {
        let items_start = start + 1;
        if row >= items_start && row < items_start + section.items.len() {
            return Some((section_index, row - items_start));
        }
        start = items_start + section.items.len();
    }
    None
}

pub(super) fn draw(
    f: &mut Frame,
    nav: &mut NavState,
    bs: &BlockStyle<'_>,
    title: &str,
    mouse: Option<(u16, u16)>,
    area: Rect,
) {
    let colors = bs.colors;
    let muted_style = Style::default().fg(colors.muted);
    let block = StyledBlock::from_color(bs, bs.colors.bg).title(title, bs.colors);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // The list widget scrolled to this offset last frame; mouse rows are mapped
    // against it before this frame's list render can adjust it.
    let previous_scroll = nav.scroll_y;

    // Cleared until the list renders, so a frame without a sidebar cannot
    // leave stale hit-test geometry behind.
    nav.list_area = Rect::default();
    nav.scroll_y = 0;
    nav.top_area = Rect::default();
    nav.top_hits.clear();
    if nav.sections.is_empty() {
        return;
    }

    let hovered = mouse.and_then(|(x, y)| {
        if !inner.contains((x, y).into()) {
            return None;
        }
        item_at_row(&nav.sections, previous_scroll + (y - inner.y) as usize)
    });

    let total_rows: usize = nav.sections.iter().map(|s| s.items.len() + 1).sum();
    let mut list_items = Vec::with_capacity(total_rows);
    let mut global_selected_idx = None;
    let mut current_global_row = 0;

    for (i, section) in nav.sections.iter().enumerate() {
        let focused = i == nav.focus_section;
        list_items.push(ListItem::new(Line::from(styled_text::parse_styled(
            &section.title,
            colors,
        ))));
        current_global_row += 1;

        let state = &nav.section_states[i];
        for (idx, item) in section.items.iter().enumerate() {
            let is_selected = state.selected() == Some(idx);
            if is_selected && focused {
                global_selected_idx = Some(current_global_row);
            }

            let name_spans = styled_text::parse_styled(&item.name, colors);
            let selected_here = is_selected && focused;
            let hovered_here = hovered == Some((i, idx));
            let line = if selected_here || hovered_here {
                // Selected items use the accent; hover uses the dimmer hover
                // colour but keeps the same configurable capsule shape.
                let (fill, fg) = if selected_here {
                    (colors.accent, colors.surface)
                } else {
                    (colors.hover(), colors.text)
                };
                let capsule = Style::default()
                    .bg(fill)
                    .fg(fg)
                    .add_modifier(if selected_here {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    });
                let name_width: usize = name_spans
                    .iter()
                    .map(|span| UnicodeWidthStr::width(&*span.content))
                    .sum();
                let left_w = UnicodeWidthStr::width(nav.selected_left.as_str());
                let right_w = UnicodeWidthStr::width(nav.selected_right.as_str());
                let padding_len =
                    (inner.width as usize).saturating_sub(left_w + 1 + name_width + right_w);
                let mut line = Line::default();
                if !nav.selected_left.is_empty() {
                    line.push_span(Span::styled(
                        nav.selected_left.as_str(),
                        Style::default().fg(fill),
                    ));
                }
                line.push_span(Span::styled(" ", capsule));
                for span in name_spans {
                    line.push_span(Span::styled(span.content, capsule));
                }
                line.push_span(Span::styled(" ".repeat(padding_len), capsule));
                if !nav.selected_right.is_empty() {
                    line.push_span(Span::styled(
                        nav.selected_right.as_str(),
                        Style::default().fg(fill),
                    ));
                }
                line
            } else {
                let mut line = Line::default();
                line.push_span(Span::styled("  ", Style::default().fg(colors.muted)));
                for span in name_spans {
                    line.push_span(Span::styled(span.content, muted_style.patch(span.style)));
                }
                line
            };

            list_items.push(ListItem::new(line));
            current_global_row += 1;
        }
    }

    let list = List::new(list_items);
    let mut global_state =
        ratatui::widgets::ListState::default().with_selected(global_selected_idx);
    f.render_stateful_widget(list, inner, &mut global_state);

    // The list widget owns the scroll offset; keep it for mouse hit-testing.
    nav.list_area = inner;
    nav.scroll_y = global_state.offset();
}

/// Top/bottom navigation mode: all items across sections are laid out in a single row, the
/// selected item is highlighted with the accent background plus side join characters, and when
/// too wide it scrolls horizontally per `NavState::scroll_x` so the selected item stays
/// visible.
pub(super) fn draw_top(
    f: &mut Frame,
    nav: &mut NavState,
    bs: &BlockStyle<'_>,
    mouse: Option<(u16, u16)>,
    area: Rect,
) {
    let colors = bs.colors;
    // One clone per frame keeps the capsule spans alive while `top_hits` is
    // mutated below.
    let selected_left = nav.selected_left.clone();
    let selected_right = nav.selected_right.clone();
    let muted_style = Style::default().fg(colors.muted);
    let block = StyledBlock::from_color(bs, bs.colors.surface);
    let inner = block.inner(area);
    f.render_widget(block, area);

    nav.list_area = Rect::default();
    nav.scroll_y = 0;
    nav.top_area = inner;
    nav.top_hits.clear();

    let left_w = UnicodeWidthStr::width(selected_left.as_str());
    let right_w = UnicodeWidthStr::width(selected_right.as_str());
    let selected = nav.selected_index().map(|index| (nav.focus_section, index));

    // Probe the layout with only the real selection decorated. The pointer is
    // interpreted against the item positions the user saw last frame, so a
    // hovered item growing its capsule cannot move the target out from under
    // the cursor.
    let (probe, _) = top_layout(nav, colors, left_w, right_w, |section, item| {
        selected == Some((section, item))
    });
    let hovered = mouse.and_then(|(x, y)| {
        if !inner.contains((x, y).into()) {
            return None;
        }
        let column = (x - inner.x) as usize + nav.scroll_x as usize;
        probe
            .iter()
            .find(|(start, width, _, _)| *start <= column && column < start + width)
            .map(|(_, _, section, item)| (*section, *item))
    });

    // Both the selected and the hovered item use the configured capsule, so a
    // hover keeps the same shape as selection.
    let (layout, total) = top_layout(nav, colors, left_w, right_w, |section, item| {
        selected == Some((section, item)) || hovered == Some((section, item))
    });

    let viewport = inner.width as usize;
    let mut line = Line::default();
    let mut selected_start = None;
    let mut selected_width = 0usize;

    for (n, &(start, chip_width, si, ii)) in layout.iter().enumerate() {
        if n > 0 {
            line.push_span(Span::styled("  ", Style::default()));
        }
        let is_selected = selected == Some((si, ii));
        let is_hovered = hovered == Some((si, ii));
        let is_active = is_selected || is_hovered;
        if is_selected {
            selected_start = Some(start);
            selected_width = chip_width;
        }

        let (fill, fg) = if is_selected {
            (colors.accent, colors.surface)
        } else {
            (colors.hover(), colors.text)
        };
        let emphasis = if is_selected {
            Modifier::BOLD
        } else {
            Modifier::empty()
        };

        if is_active {
            let capsule = Style::default().bg(fill).fg(fg).add_modifier(emphasis);
            if !selected_left.is_empty() {
                line.push_span(Span::styled(
                    selected_left.as_str(),
                    Style::default().fg(fill),
                ));
            }
            line.push_span(Span::styled(" ", capsule));
        }

        for span in styled_text::parse_styled(&nav.sections[si].items[ii].name, colors) {
            let style = if is_active {
                span.style.bg(fill).fg(fg).add_modifier(emphasis)
            } else {
                muted_style.patch(span.style)
            };
            line.push_span(Span::styled(span.content, style));
        }
        if is_active {
            let capsule = Style::default().bg(fill).fg(fg).add_modifier(emphasis);
            line.push_span(Span::styled(" ", capsule));
            if !selected_right.is_empty() {
                line.push_span(Span::styled(
                    selected_right.as_str(),
                    Style::default().fg(fill),
                ));
            }
        }
        nav.top_hits.push((start..start + chip_width, si, ii));
    }

    let max_scroll = total.saturating_sub(viewport);
    if let Some(start) = selected_start {
        nav.scroll_x = keep_visible(
            nav.scroll_x as usize,
            start,
            selected_width,
            viewport,
            max_scroll,
        ) as u16;
    } else {
        nav.scroll_x = nav.scroll_x.min(max_scroll as u16);
    }

    f.render_widget(Paragraph::new(line).scroll((0, nav.scroll_x)), inner);
}

/// `(start, chip width, section, item)` for every top-nav item, using the
/// configured capsule width for items accepted by `active`.
fn top_layout(
    nav: &NavState,
    colors: &Theme,
    left_w: usize,
    right_w: usize,
    active: impl Fn(usize, usize) -> bool,
) -> (Vec<(usize, usize, usize, usize)>, usize) {
    let mut layout = Vec::with_capacity(nav.sections.iter().map(|s| s.items.len()).sum());
    let mut total = 0usize;
    for (si, section) in nav.sections.iter().enumerate() {
        for (ii, item) in section.items.iter().enumerate() {
            if total > 0 {
                total += 2;
            }
            let name_width: usize = styled_text::parse_styled(&item.name, colors)
                .iter()
                .map(|span| UnicodeWidthStr::width(&*span.content))
                .sum();
            let width = if active(si, ii) {
                left_w + 1 + name_width + 1 + right_w
            } else {
                name_width
            };
            layout.push((total, width, si, ii));
            total += width;
        }
    }
    (layout, total)
}

/// within `viewport`, keeping the current scroll position stable whenever possible (only
/// scrolling when the selection goes out of bounds).
pub(super) fn keep_visible(
    scroll: usize,
    start: usize,
    width: usize,
    viewport: usize,
    max_scroll: usize,
) -> usize {
    if viewport == 0 {
        return 0;
    }
    let mut s = if start < scroll { start } else { scroll };
    if start + width > s + viewport {
        s = start + width - viewport;
    }
    s.min(max_scroll)
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Style};

    use super::{BlockStyle, draw, draw_top, keep_visible, styled_text};
    use crate::{
        config::{BorderConfig, NavConfig, NavItemConfig, NavSectionConfig, Theme},
        state::NavState,
    };

    #[test]
    fn unselected_item_tag_wins_over_muted_default() {
        let theme = Theme::default();
        let muted_style = Style::default().fg(theme.muted);
        let name_spans =
            styled_text::parse_styled_with("<accent>歌单</accent>", &theme, Style::default());
        // unselected items use patch() so an explicit color tag survives
        let fg = muted_style.patch(name_spans[0].style).fg;
        assert_eq!(fg, Some(theme.accent));
    }

    #[test]
    fn unselected_plain_item_falls_back_to_muted() {
        let theme = Theme::default();
        let muted_style = Style::default().fg(theme.muted);
        let name_spans = styled_text::parse_styled_with("歌单", &theme, Style::default());
        let fg = muted_style.patch(name_spans[0].style).fg;
        assert_eq!(fg, Some(theme.muted));
    }

    #[test]
    fn content_fits_viewport_no_scroll() {
        assert_eq!(keep_visible(0, 5, 4, 40, 0), 0);
    }

    #[test]
    fn selected_beyond_right_edge_scrolls_forward() {
        assert_eq!(keep_visible(0, 30, 4, 20, 100), 14);
    }

    #[test]
    fn selected_before_scroll_scrolls_back() {
        assert_eq!(keep_visible(40, 5, 4, 20, 100), 5);
    }

    #[test]
    fn clamps_to_max_scroll() {
        assert_eq!(keep_visible(0, 90, 4, 20, 80), 74);
    }

    #[test]
    fn already_visible_keeps_position() {
        assert_eq!(keep_visible(10, 12, 4, 20, 100), 10);
    }

    #[test]
    fn zero_viewport() {
        assert_eq!(keep_visible(5, 5, 4, 0, 100), 0);
    }

    fn item(name: &str) -> NavItemConfig {
        NavItemConfig {
            name: name.to_string(),
            api: Some(name.to_string()),
            title_template: None,
        }
    }

    #[test]
    fn borderless_sidebar_keeps_the_original_list() {
        let theme = Theme::default();
        let border = BorderConfig {
            enabled: false,
            ..BorderConfig::default()
        };
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let config = NavConfig {
            sections: vec![NavSectionConfig {
                title: "A".into(),
                items: vec![item("one"), item("two")],
            }],
            ..NavConfig::default()
        };
        let mut nav = NavState::from_config(&config);
        let mut term = Terminal::new(TestBackend::new(30, 6)).unwrap();
        term.draw(|f| {
            draw(f, &mut nav, &bs, "NAV", None, Rect::new(0, 0, 30, 6));
        })
        .unwrap();
        let buffer = term.backend().buffer();
        // Borderless blocks keep their one-row top padding, then the list draws
        // the section title, followed by the selected item's accent capsule.
        assert_eq!(buffer[(1, 1)].symbol(), "A", "section title row is kept");
        assert_eq!(
            buffer[(2, 2)].style().bg,
            Some(theme.accent),
            "the selected item keeps the accent capsule"
        );
        assert_eq!(
            buffer[(3, 3)].style().bg,
            Some(theme.bg),
            "unselected items are not painted as cards"
        );
    }

    #[test]
    fn selected_padding_only_decorates_the_selected_item() {
        let theme = Theme::default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let config = NavConfig {
            sections: vec![NavSectionConfig {
                title: "A".into(),
                items: vec![item("one"), item("two")],
            }],
            selected_left: "[".into(),
            selected_right: "]".into(),
        };
        let mut nav = NavState::from_config(&config);
        let mut term = Terminal::new(TestBackend::new(40, 3)).unwrap();
        term.draw(|f| draw_top(f, &mut nav, &bs, None, Rect::new(0, 0, 40, 3)))
            .unwrap();
        let count = |symbol: &str| {
            term.backend()
                .buffer()
                .content()
                .iter()
                .filter(|cell| cell.symbol() == symbol)
                .count()
        };
        assert_eq!(count("["), 1, "only the selected item gets the left pad");
        assert_eq!(count("]"), 1, "only the selected item gets the right pad");
    }

    #[test]
    fn top_nav_records_clickable_item_columns() {
        let theme = Theme::default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let config = NavConfig {
            sections: vec![
                NavSectionConfig {
                    title: "A".into(),
                    items: vec![item("one"), item("two")],
                },
                NavSectionConfig {
                    title: "B".into(),
                    items: vec![item("three")],
                },
            ],
            ..NavConfig::default()
        };
        let mut nav = NavState::from_config(&config);
        let mut term = Terminal::new(TestBackend::new(40, 3)).unwrap();
        term.draw(|f| draw_top(f, &mut nav, &bs, None, Rect::new(0, 0, 40, 3)))
            .unwrap();

        // The selected item is four cells wider because of the capsule, the
        // rest are just their name width.
        let hits = &nav.top_hits;
        assert_eq!(hits[0], (0..7, 0, 0));
        assert_eq!(hits[1], (9..12, 0, 1));
        assert_eq!(hits[2], (14..19, 1, 0));
    }

    #[test]
    fn hovered_sidebar_item_uses_the_selected_capsule() {
        let theme = Theme::default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let config = NavConfig {
            sections: vec![NavSectionConfig {
                title: "A".into(),
                items: vec![item("one"), item("two")],
            }],
            selected_left: "[".into(),
            selected_right: "]".into(),
        };
        let mut nav = NavState::from_config(&config);
        let mut term = Terminal::new(TestBackend::new(40, 6)).unwrap();
        term.draw(|f| {
            draw(
                f,
                &mut nav,
                &bs,
                "NAV",
                Some((2, 3)),
                Rect::new(0, 0, 40, 6),
            );
        })
        .unwrap();
        let buffer = term.backend().buffer();
        // Row 2 holds the selected "one"; row 3 is hovered.
        assert_eq!(buffer[(1, 2)].symbol(), "[");
        assert_eq!(buffer[(1, 3)].symbol(), "[", "hover uses the left pad");
        assert_eq!(
            buffer[(2, 3)].style().bg,
            Some(theme.hover()),
            "hover uses the hover fill"
        );
        assert_ne!(
            buffer[(2, 3)].style().bg,
            Some(theme.accent),
            "hover is visibly different from the selection"
        );
        assert_eq!(
            buffer[(2, 2)].style().bg,
            Some(theme.accent),
            "the real selection keeps its capsule"
        );
    }

    #[test]
    fn hovered_top_nav_item_uses_the_selected_capsule() {
        let theme = Theme::default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };
        let config = NavConfig {
            sections: vec![NavSectionConfig {
                title: "A".into(),
                items: vec![item("one"), item("two")],
            }],
            selected_left: "[".into(),
            selected_right: "]".into(),
        };
        let mut nav = NavState::from_config(&config);
        let mut term = Terminal::new(TestBackend::new(40, 3)).unwrap();
        term.draw(|f| {
            draw_top(f, &mut nav, &bs, Some((11, 1)), Rect::new(0, 0, 40, 3));
        })
        .unwrap();
        let buffer = term.backend().buffer();
        // The selected "one" occupies columns 0..7 of the line; "two" starts
        // at column 9, so its hover capsule's left glyph is at screen x = 10.
        assert_eq!(buffer[(1, 1)].symbol(), "[");
        assert_eq!(buffer[(10, 1)].symbol(), "[", "hover uses the left pad");
        assert_eq!(
            buffer[(12, 1)].style().bg,
            Some(theme.hover()),
            "hover uses the hover fill"
        );
        assert_eq!(
            buffer[(3, 1)].style().bg,
            Some(theme.accent),
            "the selected item keeps the accent fill"
        );
    }
}
