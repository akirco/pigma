use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::BlockStyle;
use super::{create_block, create_block_surfaced, styled_text};
use crate::state::NavState;

pub fn draw(f: &mut Frame, nav: &mut NavState, bs: &BlockStyle<'_>, title: &str, area: Rect) {
    let colors = bs.colors;
    let muted_style = Style::default().fg(colors.muted);
    let block = create_block(title, bs, false);
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

            let name_spans = styled_text::parse_styled(&item.name, colors);

            let line = if is_selected && focused {
                let capsule = Style::default()
                    .bg(colors.accent)
                    .fg(colors.surface)
                    .add_modifier(Modifier::BOLD);
                let name_width: usize = name_spans
                    .iter()
                    .map(|s| UnicodeWidthStr::width(&*s.content))
                    .sum();
                let padding_len = (inner.width as usize).saturating_sub(3 + name_width);
                let mut line = Line::default();
                line.push_span(Span::styled("\u{E0B2}", Style::default().fg(colors.accent)));
                line.push_span(Span::styled(" ", capsule));
                for s in name_spans {
                    line.push_span(Span::styled(s.content, capsule));
                }
                line.push_span(Span::styled(" ".repeat(padding_len), capsule));
                line.push_span(Span::styled("\u{E0B0}", Style::default().fg(colors.accent)));
                line
            } else {
                let mut line = Line::default();
                line.push_span(Span::styled("  ", Style::default().fg(colors.muted)));
                for s in name_spans {
                    line.push_span(Span::styled(s.content, muted_style.patch(s.style)));
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
}

/// 顶部/底部导航模式：所有 item 跨 section 平铺成一行，选中项以 accent 底色
/// 高亮并带左右拼接字符，超宽时按 `NavState::scroll_x` 横向滚动，保证选中项
/// 始终可见。
pub fn draw_top(f: &mut Frame, nav: &mut NavState, bs: &BlockStyle<'_>, area: Rect) {
    let colors = bs.colors;
    let muted_style = Style::default().fg(colors.muted);
    let block = create_block_surfaced("", bs, false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let viewport = inner.width as usize;

    let mut line = Line::default();
    let mut total = 0usize;
    let mut selected_start = None;
    let mut selected_width = 0usize;

    for (si, section) in nav.sections.iter().enumerate() {
        for (ii, item) in section.items.iter().enumerate() {
            let is_selected =
                nav.focus_section == si && nav.section_states[si].selected() == Some(ii);

            if !line.spans.is_empty() {
                line.push_span(Span::styled("  ", Style::default()));
                total += 2;
            }

            let name_spans = styled_text::parse_styled(&item.name, colors);
            let width: usize = name_spans
                .iter()
                .map(|s| UnicodeWidthStr::width(&*s.content))
                .sum();

            if is_selected {
                let capsule = Style::default()
                    .bg(colors.accent)
                    .fg(colors.surface)
                    .add_modifier(Modifier::BOLD);
                selected_start = Some(total);
                selected_width = width + 4;
                line.push_span(Span::styled("\u{E0B2}", Style::default().fg(colors.accent)));
                line.push_span(Span::styled(" ", capsule));
            }

            for s in name_spans {
                let style = if is_selected {
                    s.style
                        .bg(colors.accent)
                        .fg(colors.surface)
                        .add_modifier(Modifier::BOLD)
                } else {
                    muted_style.patch(s.style)
                };
                line.push_span(Span::styled(s.content, style));
            }
            if is_selected {
                let capsule = Style::default()
                    .bg(colors.accent)
                    .fg(colors.surface)
                    .add_modifier(Modifier::BOLD);
                line.push_span(Span::styled(" ", capsule));
                line.push_span(Span::styled("\u{E0B0}", Style::default().fg(colors.accent)));
            }
            total += width + if is_selected { 4 } else { 0 };
        }
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

/// 计算横向滚动偏移，使 `[start, start+width)` 的选中项在 `viewport` 内可见，
/// 且尽量保持当前滚动位置不动（仅当选中项越界时才滚动）。
pub fn keep_visible(
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
    use super::keep_visible;
    use crate::config::Theme;
    use crate::ui::styled_text;
    use ratatui::style::Style;

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
}
