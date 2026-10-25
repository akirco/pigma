//! Main screen area layout: splits the frame into topbar, navigation, content and
//! player-bar regions (plus the splash layout).

use ratatui::layout::{Constraint, Flex, Layout, Rect};

use crate::{config::NavPosition, state::Page};

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
    pub logo: Rect,
    pub login_box: Rect,
}

pub fn login(area: Rect) -> LoginLayout {
    let [status_area, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(26)])
        .flex(Flex::Center)
        .spacing(1)
        .areas(area);

    let [logo_area, box_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(body);

    LoginLayout {
        status: status_area,
        logo: logo_area,
        login_box: box_area,
    }
}

pub struct LayoutAreas {
    pub topbar: Rect,
    pub sidebar: Rect,
    pub breadcrumb: Rect,
    pub nav: Rect,
    pub content: Rect,
    pub playerbar: Rect,
}

pub fn build_layout(area: Rect, page: Page, nav_position: NavPosition) -> LayoutAreas {
    match page {
        Page::Main => match nav_position {
            NavPosition::Top => {
                let [topbar, nav, middle, playerbar] = Layout::vertical([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(10),
                    Constraint::Length(5),
                ])
                .areas(area);

                LayoutAreas {
                    topbar,
                    sidebar: Rect::default(),
                    breadcrumb: Rect::default(),
                    nav,
                    content: middle,
                    playerbar,
                }
            }
            NavPosition::Bottom => {
                let [topbar, middle, nav, playerbar] = Layout::vertical([
                    Constraint::Length(3),
                    Constraint::Min(10),
                    Constraint::Length(3),
                    Constraint::Length(5),
                ])
                .areas(area);

                LayoutAreas {
                    topbar,
                    sidebar: Rect::default(),
                    breadcrumb: Rect::default(),
                    nav,
                    content: middle,
                    playerbar,
                }
            }
            NavPosition::Left | NavPosition::Right => {
                let [topbar, middle, playerbar] = Layout::vertical([
                    Constraint::Length(3),
                    Constraint::Min(10),
                    Constraint::Length(5),
                ])
                .areas(area);

                // Hide the sidebar when the terminal is narrower than 60 columns; content fills the whole area
                let (sidebar, right) = if area.width < 60 {
                    (Rect::default(), middle)
                } else {
                    match nav_position {
                        NavPosition::Left => {
                            let [sidebar, right] =
                                Layout::horizontal([Constraint::Length(26), Constraint::Min(40)])
                                    .areas(middle);
                            (sidebar, right)
                        }
                        NavPosition::Right => {
                            let [right, sidebar] =
                                Layout::horizontal([Constraint::Min(40), Constraint::Length(26)])
                                    .areas(middle);
                            (sidebar, right)
                        }
                        _ => unreachable!(),
                    }
                };

                let [breadcrumb, content] =
                    Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(right);

                LayoutAreas {
                    topbar,
                    sidebar,
                    breadcrumb,
                    nav: Rect::default(),
                    content,
                    playerbar,
                }
            }
        },
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
                nav: Rect::default(),
                content: middle,
                playerbar,
            }
        }
        _ => LayoutAreas {
            topbar: Rect::default(),
            sidebar: Rect::default(),
            breadcrumb: Rect::default(),
            nav: Rect::default(),
            content: area,
            playerbar: Rect::default(),
        },
    }
}
