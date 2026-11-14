//! Top-level frame rendering: picks the theme, dispatches on the active page
//! and overlays the command panel, help view and toast.

use std::time::Duration;

use ratatui::{Frame, layout::Rect};

use crate::{
    app::App,
    config::{Config, NavPosition},
    state::{NavigationState, Page},
};

use super::{
    BlockStyle, block::StyledBlock, breadcrumb, command_panel, content, equalizer, help, layout,
    login, lyrics, navigation, playerbar, queue, splash, title, toast, topbar,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let now = std::time::Instant::now();
    let steps = (now.duration_since(app.state.last_tick).as_millis() / 80).max(1) as u64;
    app.state.last_tick = now;
    app.state.tick = app.state.tick.wrapping_add(steps);

    if let Some(t) = app.state.toast_time
        && t.elapsed() > Duration::from_secs(2)
    {
        app.state.toast_time = None;
    }

    let area = f.area();

    let colors = app.theme_registry.resolve(&app.config.default_theme);

    let bs = BlockStyle {
        colors,
        border: &app.state.border,
        tick: app.state.tick,
    };

    let content_covers = app.content_covers();
    let font_size = app.picker_font_size();
    let mut playerbar_rect = Rect::default();
    match app.state.navigation.page {
        Page::Splash => {
            let lay = layout::splash(area);
            splash::draw(f, &app.state.splash, &bs, &lay);
        }
        Page::Login => {
            let lay = layout::login(area);
            login::draw(f, &mut app.state.login, &bs, &lay);
        }
        page => {
            let lay = layout::build_layout(area, page, app.config.navigation_position);

            topbar::draw(
                f,
                app.state.navigation.user.as_ref(),
                &app.state.navigation.search,
                &bs,
                lay.topbar,
            );
            playerbar_rect = lay.playerbar;
            let is_sixel = app.picker_protocol() == ratatui_image::picker::ProtocolType::Sixel;
            playerbar::draw(
                f,
                &app.playback.state,
                app.state.tick,
                &bs,
                &app.config.playerbar,
                lay.playerbar,
                is_sixel,
            );

            match page {
                Page::Main => draw_main(
                    f,
                    &mut app.state.navigation,
                    &app.config,
                    &bs,
                    &lay,
                    &content_covers,
                    font_size,
                ),
                Page::Lyrics => {
                    lyrics::draw(
                        f,
                        &app.playback.state,
                        &bs,
                        app.config.lyric_gradient,
                        &app.config.titles.lyrics,
                        lay.content,
                    );
                }
                Page::Playlist => {
                    queue::draw_queue_table(
                        f,
                        &app.playback,
                        app.state.navigation.playlist_selected,
                        &bs,
                        &app.config.titles.playlist,
                        &mut app.state.navigation.queue_tab_scroll_x,
                        lay.content,
                    );
                }
                Page::Equalizer => {
                    equalizer::draw(
                        f,
                        &app.playback.state.equalizer,
                        app.state.eq_selected,
                        &bs,
                        lay.content,
                    );
                }
                _ => {}
            }
        }
    }

    if app.state.command_panel.open {
        command_panel::draw(
            f,
            &app.state.command_panel,
            &app.config.default_theme,
            app.config.cache.save_on_play,
            &bs,
            area,
        );
    }

    if app.state.help.open {
        // Persist the rendered scroll limit so scroll_down clamps at the real
        // bottom; clamping scroll here also heals drift after a resize.
        let max_scroll = help::draw(f, app.state.help.scroll, &bs, area);
        app.state.help.max_scroll = max_scroll;
        app.state.help.scroll = app.state.help.scroll.min(max_scroll);
    }

    if let Some(time) = app.state.toast_time {
        toast::draw_toast(f, &app.state.toast_msg, time.elapsed(), colors);
    }
    // Recorded after `colors` (borrowed from `app.theme_registry`) is last
    // used: the setter takes `&mut App` and cannot overlap that borrow.
    app.set_playerbar_area(playerbar_rect);
    app.prefetch_content_covers();
}

/// Main content page: navigation sidebar/topbar, content title block and table.
fn draw_main(
    f: &mut Frame,
    nav: &mut NavigationState,
    config: &Config,
    bs: &BlockStyle<'_>,
    lay: &layout::LayoutAreas,
    covers: &crate::app::cover::ContentCovers,
    font_size: ratatui_image::FontSize,
) {
    match config.navigation_position {
        NavPosition::Left | NavPosition::Right => {
            if lay.sidebar.width > 0 {
                navigation::draw(f, &mut nav.nav, bs, &config.titles.sidebar, lay.sidebar);
            }

            breadcrumb::render_breadcrumb(f, &nav.nav, bs, lay.breadcrumb);
        }
        NavPosition::Top | NavPosition::Bottom => {
            navigation::draw_top(f, &mut nav.nav, bs, lay.nav);
        }
    }

    let title = title::content_title(nav);
    let block = StyledBlock::from_color(bs, bs.colors.bg).title(&title, bs.colors);
    let inner = block.inner(lay.content);
    f.render_widget(block, lay.content);

    let api = nav.nav.selected_api();
    content::render_content(
        f,
        &nav.content,
        &config.columns,
        api,
        bs,
        &mut nav.table_state,
        nav.content_selected,
        nav.table_mode,
        &config.content,
        &mut nav.content_columns,
        &mut nav.content_scroll,
        &mut nav.content_visible,
        covers,
        font_size,
        inner,
    );
}
