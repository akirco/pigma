use ratatui::layout::{Constraint, Flex, Layout, Rect};

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

pub struct MainLayout {
    pub topbar: Rect,
    pub nav: Rect,
    pub songs: Rect,
    pub body: Rect,
    pub playerbar: Rect,
}

pub fn main(area: Rect) -> MainLayout {
    let [topbar_area, body, playerbar_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(5),
    ])
    .areas(area);

    let [nav_area, songs_area] =
        Layout::horizontal([Constraint::Length(26), Constraint::Min(40)]).areas(body);

    MainLayout {
        topbar: topbar_area,
        nav: nav_area,
        songs: songs_area,
        body,
        playerbar: playerbar_area,
    }
}
