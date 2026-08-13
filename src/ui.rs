//! ratatui widgets and rendering helpers used by the views (tables, player bar,
//! navigation, lyrics, toasts, spinners, breadcrumbs, ...).

mod block;
mod breadcrumb;
mod command_panel;
mod content;
mod gradient_line_gauge;
mod help;
mod login;
mod lyrics;
mod navigation;
mod playerbar;
mod queue;
mod scrollbar;
mod skeleton;
mod spinner;
mod splash;
mod styled_text;
mod table;
mod title;
mod toast;
mod topbar;

use std::sync::Arc;
use std::time::Duration;

use ratatui::Frame;

use crate::app::App;
use crate::config::NavPosition;
use crate::layout;
use crate::state::Page;
use crate::ui::block::{BlockStyle, CornerBlock};
use crate::ui::title::render_title;

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

    let colors = App::resolve_theme(&app.config, &app.theme_registry);

    let bs = BlockStyle {
        colors,
        border: &app.state.border,
        tick: app.state.tick,
    };

    match app.state.navigation.page {
        Page::Splash => {
            let lay = layout::splash(area);
            splash::draw(f, &app.state.splash, &bs, &lay);
        }
        Page::Login => {
            let lay = layout::login(area);
            login::draw(f, &app.state.login, &bs, &lay);
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
            app.state.playerbar_area = lay.playerbar;
            let is_sixel = app.picker.protocol_type() == ratatui_image::picker::ProtocolType::Sixel;
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
                Page::Main => {
                    match app.config.navigation_position {
                        NavPosition::Left | NavPosition::Right => {
                            if lay.sidebar.width > 0 {
                                navigation::draw(
                                    f,
                                    &mut app.state.navigation.nav,
                                    &bs,
                                    &app.config.titles.sidebar,
                                    lay.sidebar,
                                );
                            }

                            breadcrumb::render_breadcrumb(
                                f,
                                &app.state.navigation.nav,
                                &bs,
                                lay.breadcrumb,
                            );
                        }
                        NavPosition::Top | NavPosition::Bottom => {
                            navigation::draw_top(f, &mut app.state.navigation.nav, &bs, lay.nav);
                        }
                    }

                    let nav = &app.state.navigation.nav;
                    let current_item = nav.selected_item();

                    let title = {
                        let nst = &app.state.navigation;
                        let focus = nst.nav.focus_section;
                        let selected = nst.nav.selected_index();
                        let generation = nst.generation;
                        let count = nst.content.len();
                        let cached = nst.title_cache.borrow();
                        if let Some((ref title, f, s, g, c)) = *cached
                            && f == focus
                            && s == selected
                            && g == generation
                            && c == count
                        {
                            Arc::clone(title)
                        } else {
                            drop(cached);
                            let name = current_item
                                .map(|item| item.name.as_str())
                                .unwrap_or("SONGS");
                            let total = nst
                                .pagination
                                .as_ref()
                                .map(|p| p.total as usize)
                                .unwrap_or(count);
                            // When content is paged (total known), show `count/total` like the
                            // music cloud drive; non-paged content shows only the loaded count.
                            // Each nav item can also override this via `title_template`.
                            let show_total = nst.pagination.as_ref().is_some_and(|p| p.total > 0);
                            let template = current_item
                                .and_then(|item| item.title_template.as_deref())
                                .unwrap_or(if show_total {
                                    "\u{25BA} {name} ({count}/{total}) \u{25C4}"
                                } else {
                                    "\u{25BA} {name} ({count}) \u{25C4}"
                                });
                            let title = Arc::new(render_title(template, name, count, total));
                            *nst.title_cache.borrow_mut() =
                                Some((Arc::clone(&title), focus, selected, generation, count));
                            title
                        }
                    };
                    let block = CornerBlock::from_color(&bs, bs.colors.bg).title(&title, bs.colors);
                    let inner = block.inner(lay.content);
                    f.render_widget(block, lay.content);

                    let api = nav.selected_api();

                    content::render_content(
                        f,
                        &app.state.navigation.content,
                        &app.config.columns,
                        api,
                        &bs,
                        &mut app.state.navigation.table_state,
                        app.state.navigation.table_mode,
                        inner,
                    );
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
                _ => {}
            }
        }
    }

    if app.state.command_panel.open {
        command_panel::draw(f, app, area);
    }

    if app.state.help.open {
        help::draw(f, app, area);
    }

    toast::draw_toast(f, app, colors);
}
