use ratatui::layout::{Constraint, Flex, Layout, Rect};

use crate::state::Page;

pub struct SplashLayout {
    pub logo: Rect,
    pub progress: Rect,
    pub logs: Rect,
    pub tag: Rect,
}

pub fn splash(area: Rect) -> SplashLayout {
    let [logo_area, progress_area, logs_area, tag_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Length(5),
        Constraint::Length(1),
    ])
    .flex(Flex::SpaceAround)
    .areas(area);

    SplashLayout {
        logo: logo_area,
        progress: progress_area,
        logs: logs_area,
        tag: tag_area,
    }
}

pub struct LoginLayout {
    pub status: Rect,
    pub login_box: Rect,
}

pub fn login(area: Rect) -> LoginLayout {
    let [status_area, box_area] = Layout::vertical([Constraint::Length(1), Constraint::Min(26)])
        .flex(Flex::Center)
        .spacing(1)
        .areas(area);

    LoginLayout {
        status: status_area,
        login_box: box_area,
    }
}

pub struct LayoutAreas {
    pub topbar: Rect,
    pub sidebar: Rect,
    pub breadcrumb: Rect,
    pub content: Rect,
    pub playerbar: Rect,
}

pub fn build_layout(area: Rect, page: Page) -> LayoutAreas {
    match page {
        Page::Main => {
            let [topbar, middle, playerbar] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(5),
            ])
            .areas(area);

            let [sidebar, right] = Layout::horizontal([
                Constraint::Length(26),
                Constraint::Min(40),
            ])
            .areas(middle);

            let [breadcrumb, content] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(1),
            ])
            .areas(right);

            LayoutAreas {
                topbar,
                sidebar,
                breadcrumb,
                content,
                playerbar,
            }
        }
        Page::Lyrics | Page::Playlist => {
            let [topbar, middle, playerbar] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(5),
            ])
            .areas(area);

            LayoutAreas {
                topbar,
                sidebar: Rect::default(),
                breadcrumb: Rect::default(),
                content: middle,
                playerbar,
            }
        }
        _ => LayoutAreas {
            topbar: Rect::default(),
            sidebar: Rect::default(),
            breadcrumb: Rect::default(),
            content: area,
            playerbar: Rect::default(),
        },
    }
}
