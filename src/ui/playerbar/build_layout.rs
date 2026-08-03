use ratatui::layout::{Constraint, Flex, Layout, Rect};

#[derive(Debug, Clone, Default)]
pub struct LayoutArea {
    pub progress_time_left: Rect,
    pub progress_bar: Rect,
    pub progress_time_right: Rect,
    pub song_info: Rect,
    pub song_detail: Rect,
    pub cover: Rect,
    pub controls: Rect,
    pub gauge: Rect,
    pub spinner: Rect,
    pub mode_icon: Rect,
    pub volume: Rect,
}

pub fn build_default(area: Rect) -> LayoutArea {
    let cols = Layout::horizontal([
        Constraint::Length(20),
        Constraint::Min(30),
        Constraint::Length(3),
        Constraint::Length(8),
    ])
    .split(area);

    let mid = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(cols[1]);

    let right = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(cols[3]);

    LayoutArea {
        song_info: cols[0],
        controls: mid[0],
        gauge: mid[2],
        spinner: cols[2],
        mode_icon: right[0],
        volume: right[2],
        ..Default::default()
    }
}

pub fn build_modern(area: Rect, show_cover: bool, _show_volume: bool) -> LayoutArea {
    let cols = Layout::horizontal([
        if show_cover {
            Constraint::Length(8)
        } else {
            Constraint::Length(0)
        },
        Constraint::Min(20),
    ])
    .spacing(1)
    .split(area);

    let cover_area = cols[0];

    let right_rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .horizontal_margin(1)
    .split(cols[1]);

    let progress_cols = Layout::horizontal([
        Constraint::Length(6),
        Constraint::Min(10),
        Constraint::Length(6),
    ])
    .split(right_rows[0]);

    // Middle: song_info(left) | spinner(right)
    let middle_cols = Layout::horizontal([Constraint::Min(10), Constraint::Length(8)])
        .flex(Flex::SpaceBetween)
        .split(right_rows[1]);

    // Bottom: song_detail(left) | controls(center) | mode(right)
    let bottom_cols = Layout::horizontal([
        Constraint::Length(15),
        Constraint::Length(20),
        Constraint::Length(6),
    ])
    .flex(Flex::SpaceBetween)
    .split(right_rows[2]);

    let vol_mode_cols =
        Layout::horizontal([Constraint::Length(3), Constraint::Length(3)]).split(bottom_cols[2]);

    LayoutArea {
        cover: cover_area,
        progress_time_left: progress_cols[0],
        progress_bar: progress_cols[1],
        progress_time_right: progress_cols[2],
        song_info: middle_cols[0],
        spinner: middle_cols[1],
        song_detail: bottom_cols[0],
        controls: bottom_cols[1],
        volume: vol_mode_cols[0],
        mode_icon: vol_mode_cols[1],
        ..Default::default()
    }
}

pub fn build_minimal(area: Rect) -> LayoutArea {
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);

    let cols = Layout::horizontal([
        Constraint::Percentage(12),
        Constraint::Length(18),
        Constraint::Length(6),
        Constraint::Min(0),
        Constraint::Length(6),
        Constraint::Length(3),
    ])
    .spacing(2)
    .split(rows[1]);

    LayoutArea {
        song_info: cols[0],
        controls: cols[1],
        progress_time_left: cols[2],
        gauge: cols[3],
        progress_time_right: cols[4],
        mode_icon: cols[5],
        ..Default::default()
    }
}
