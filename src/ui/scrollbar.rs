use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};

pub fn calc_scroll_offset(selected: usize, visible_height: usize, total: usize) -> usize {
    if total <= visible_height || visible_height == 0 {
        return 0;
    }
    if selected < visible_height {
        0
    } else {
        selected.saturating_sub(visible_height - 1)
    }
}

pub fn render_scrollbar(f: &mut Frame, total: usize, selected: usize, area: Rect) {
    let mut state = ScrollbarState::new(total).position(selected);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .thumb_symbol("│")
        .track_symbol(None);
    f.render_stateful_widget(scrollbar, area, &mut state);
}
