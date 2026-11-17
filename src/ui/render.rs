//! Top-level frame rendering: picks the theme, dispatches on the active page
//! and overlays the command panel, help view and toast.
//!
//! The render pass is read-only for application state. Widget-owned state
//! (`TableState` / `ListState`) is borrowed mutably because ratatui requires it,
//! but every geometry-derived value is returned in a [`RenderOutcome`] and
//! applied by `App::draw` at one explicit write-back point.

use std::ops::Range;

use ratatui::{Frame, layout::Rect};

use crate::{
    app::App,
    config::{Config, NavPosition},
    state::{NavigationState, Page, QueueTabHit},
};

use super::{
    BlockStyle, PlayerbarHits, block::StyledBlock, breadcrumb, cards, command_panel, content,
    equalizer, help, layout, login, lyrics, navigation, playerbar, queue, splash, title, toast,
    topbar,
};

/// Geometry-derived values produced by one frame. `None` means the matching
/// view was not drawn, so its previous value stays valid.
#[derive(Debug, Default)]
pub(crate) struct RenderOutcome {
    pub content_layout: Option<ContentLayout>,
    pub playerbar_area: Rect,
    /// Clickable player-bar controls from this frame.
    pub playerbar_hits: PlayerbarHits,
    pub help_max_scroll: Option<usize>,
    pub queue_tab_scroll_x: Option<u16>,
    /// Queue tab bar and its clickable tabs, for mouse hit-testing.
    pub queue_tabs: Option<(Rect, Vec<QueueTabHit>)>,
    /// Queue table body and its first visible song, for mouse hit-testing.
    pub queue_body: Option<(Rect, usize)>,
}

/// Layout of the content viewport, stored on `NavigationState` after the frame
/// so navigation and cover prefetch read the numbers the renderer actually used.
#[derive(Debug, Clone, Default)]
pub(crate) struct ContentLayout {
    pub columns: usize,
    pub scroll: usize,
    pub visible: Range<usize>,
    /// Item under the pointer, when the grid could map the pointer to a card.
    pub hovered: Option<usize>,
}

pub(crate) fn draw(f: &mut Frame, app: &mut App) -> RenderOutcome {
    let area = f.area();
    let mut outcome = RenderOutcome::default();

    let colors = app.theme_registry.resolve(&app.config.default_theme);

    let bs = BlockStyle {
        colors,
        border: &app.state.border,
        tick: app.state.tick,
    };

    let content_covers = app.content_covers();
    let font_size = app.picker_font_size();
    // Overlays sit on top of the page, so a stale pointer must not light up
    // controls behind them.
    let live_mouse = if app.state.help.open || app.state.command_panel.open {
        None
    } else {
        app.state.mouse
    };
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
            outcome.playerbar_area = lay.playerbar;
            let is_sixel = app.picker_protocol() == ratatui_image::picker::ProtocolType::Sixel;
            outcome.playerbar_hits = playerbar::draw(
                f,
                &app.playback.state,
                app.state.tick,
                &bs,
                &app.config.playerbar,
                lay.playerbar,
                is_sixel,
                live_mouse,
            );

            match page {
                Page::Main => {
                    // Overlays sit on top of the cards, so a stale pointer must
                    // not light one up behind them.
                    let live = cards::LiveInputs {
                        mouse: live_mouse,
                        playing_song: app.playback.current_song().map(|song| song.id),
                        playing: app.playback.state.playing,
                        tick: app.state.tick,
                    };
                    outcome.content_layout = Some(draw_main(
                        f,
                        &mut app.state.navigation,
                        &app.config,
                        &bs,
                        &lay,
                        &content_covers,
                        font_size,
                        live,
                    ));
                }
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
                    let queue = queue::draw_queue_table(
                        f,
                        &app.playback,
                        app.state.navigation.playlist_selected,
                        &bs,
                        &app.config.titles.playlist,
                        app.state.navigation.queue_tab_scroll_x,
                        lay.content,
                    );
                    outcome.queue_tab_scroll_x = Some(queue.tab_scroll);
                    outcome.queue_tabs = Some((queue.tab_area, queue.tab_hits));
                    outcome.queue_body = Some((queue.body, queue.offset));
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
        outcome.help_max_scroll = Some(help::draw(f, app.state.help.scroll, &bs, area));
    }

    if app.state.toast_time.is_some() {
        toast::draw_toast(f, &app.state.toast_msg, colors);
    }
    outcome
}

/// Main content page: navigation sidebar/topbar, content title block and table.
/// Returns the layout the renderer settled on instead of writing it back.
// Argument count is already at clippy's default limit; the card live inputs
// are one more rendering input, and grouping them into another struct here
// would only move the same fields around.
#[allow(clippy::too_many_arguments)]
fn draw_main(
    f: &mut Frame,
    nav: &mut NavigationState,
    config: &Config,
    bs: &BlockStyle<'_>,
    lay: &layout::LayoutAreas,
    covers: &crate::cache::cover::ContentCovers,
    font_size: ratatui_image::FontSize,
    live: cards::LiveInputs,
) -> ContentLayout {
    match config.navigation_position {
        NavPosition::Left | NavPosition::Right => {
            if lay.sidebar.width > 0 {
                navigation::draw(
                    f,
                    &mut nav.nav,
                    bs,
                    &config.titles.sidebar,
                    live.mouse,
                    lay.sidebar,
                );
            }

            breadcrumb::render_breadcrumb(f, &nav.nav, bs, lay.breadcrumb);
        }
        NavPosition::Top | NavPosition::Bottom => {
            navigation::draw_top(f, &mut nav.nav, bs, live.mouse, lay.nav);
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
        nav.content_scroll,
        covers,
        font_size,
        live,
        inner,
    )
}
