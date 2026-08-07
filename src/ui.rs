mod block;
mod breadcrumb;
mod command_panel;
mod content;
mod gradient_line_gauge;
mod help;
mod login;
mod lyrics;
mod navigation;
pub mod playerbar;
mod queue;
mod spinner;
mod splash;
pub mod styled_text;
pub mod table;
mod topbar;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{
        Block, BorderType, Borders, Clear, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
};
use std::time::Duration;

use self::block::CornerBlock;

use crate::{
    config::{BorderConfig, Theme},
    layout,
    state::{App, Page},
};

pub struct BlockStyle<'a> {
    pub colors: &'a Theme,
    pub border: &'a BorderConfig,
    pub tick: u64,
}

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

pub fn render_scrollbar(f: &mut Frame, total: usize, selected: usize, area: ratatui::layout::Rect) {
    let mut state = ScrollbarState::new(total).position(selected);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .thumb_symbol("│")
        .track_symbol(None);
    f.render_stateful_widget(scrollbar, area, &mut state);
}

/// Render a title template with `{name}`, `{count}`, and `{total}` placeholders.
/// `count` is the number of currently loaded items, `total` the server-side
/// total (0 when unknown / non-paginated).
pub fn render_title(template: &str, name: &str, count: usize, total: usize) -> String {
    if !template.contains('{') {
        return template.to_owned();
    }
    template
        .replace("{name}", name)
        .replace("{count}", &count.to_string())
        .replace("{total}", &total.to_string())
}

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

    let colors = app
        .theme_registry
        .get(&app.config.default_theme)
        .or_else(|| app.theme_registry.get("default"))
        .unwrap_or_else(|| {
            log::error!("No theme found, using fallback");
            crate::state::theme_fallback()
        });

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
            login::draw(f, &app.state.navigation.login, &bs, &lay);
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
                        crate::config::NavPosition::Left | crate::config::NavPosition::Right => {
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
                        crate::config::NavPosition::Top | crate::config::NavPosition::Bottom => {
                            navigation::draw_top(f, &mut app.state.navigation.nav, &bs, lay.nav);
                        }
                    }

                    let nav = &app.state.navigation.nav;
                    let current_item = nav
                        .section_states
                        .get(nav.focus_section)
                        .and_then(|st| st.selected())
                        .and_then(|i| nav.sections.get(nav.focus_section)?.items.get(i));

                    let title = {
                        let nst = &app.state.navigation;
                        let focus = nst.nav.focus_section;
                        let selected = nst
                            .nav
                            .section_states
                            .get(focus)
                            .and_then(|st| st.selected());
                        let generation = nst.generation;
                        let count = nst.content.len();
                        let cached = nst.title_cache.borrow();
                        if let Some((ref title, f, s, g, c)) = *cached
                            && f == focus
                            && s == selected
                            && g == generation
                            && c == count
                        {
                            title.clone()
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
                            // 内容是分页加载（总数已知）时，像音乐云盘那样显示 `count/total`；
                            // 非分页内容只显示已加载数。各导航项也可用 `title_template` 覆盖。
                            let show_total = nst.pagination.as_ref().is_some_and(|p| p.total > 0);
                            let template = current_item
                                .and_then(|item| item.title_template.as_deref())
                                .unwrap_or(if show_total {
                                    "\u{25BA} {name} ({count}/{total}) \u{25C4}"
                                } else {
                                    "\u{25BA} {name} ({count}) \u{25C4}"
                                });
                            let title = render_title(template, name, count, total);
                            *nst.title_cache.borrow_mut() =
                                Some((title.clone(), focus, selected, generation, count));
                            title
                        }
                    };
                    let block = create_block(&title, &bs, false);
                    let inner = block.inner(lay.content);
                    f.render_widget(block, lay.content);

                    let api = current_item.and_then(|item| item.api.as_deref());

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
                        &mut app.state.queue_tab_scroll_x,
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

    draw_toast(f, app, colors);
}

fn draw_toast(f: &mut Frame, app: &App, colors: &Theme) {
    let Some(time) = app.state.toast_time else {
        return;
    };
    if time.elapsed() > Duration::from_secs(2) {
        return;
    }

    let area = f.area();
    let display_w = unicode_width::UnicodeWidthStr::width(app.state.toast_msg.as_str());
    let w = (display_w as u16 + 6).min(area.width);
    let h = 3u16;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + area.height.saturating_sub(10);

    let toast_area = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    f.render_widget(Clear, toast_area);

    let block = Block::default()
        .borders(Borders::TOP)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors.border))
        .style(Style::default().bg(colors.surface));

    let p = Paragraph::new(format!(" {} ", app.state.toast_msg))
        .style(Style::default().fg(colors.text))
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(p, toast_area);
}

pub(crate) fn create_block<'a>(
    title: &'a str,
    style: &'a BlockStyle<'a>,
    _focused: bool,
) -> CornerBlock<'a> {
    create_block_bg(title, style, _focused, style.colors.bg)
}

pub(crate) fn create_block_surfaced<'a>(
    title: &'a str,
    style: &'a BlockStyle<'a>,
    _focused: bool,
) -> CornerBlock<'a> {
    create_block_bg(title, style, _focused, style.colors.surface)
}

fn create_block_bg<'a>(
    title: &'a str,
    style: &'a BlockStyle<'a>,
    _focused: bool,
    no_border_bg: Color,
) -> CornerBlock<'a> {
    let border_color = style.colors.border;
    let border_type = if style.border.rounded {
        BorderType::Rounded
    } else {
        BorderType::Plain
    };
    let title_line = ratatui::text::Line::from(styled_text::parse_styled(title, style.colors));
    let block = if style.border.enabled {
        Block::default()
            .borders(Borders::ALL)
            .border_type(border_type)
            .border_style(Style::default().fg(border_color))
            .title(title_line)
            .title_style(Style::default().fg(style.colors.muted))
    } else {
        Block::default()
            .borders(Borders::NONE)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(no_border_bg))
            .title(title_line)
            .title_style(Style::default().fg(style.colors.muted))
            .padding(Padding::horizontal(1))
    };
    CornerBlock::new(block)
        .corner_color(style.colors.accent)
        .corner_sizes(2, 1)
        .follow_corner_color(style.border.follow_corner_color)
        .border_gradient(style.border.border_gradient)
        .border_gradient_speed(style.border.border_gradient_speed)
        .tick(style.tick)
}

#[cfg(test)]
mod tests {
    use super::render_title;

    #[test]
    fn title_with_count_suffix() {
        assert_eq!(
            render_title("每日推荐 ({count})", "每日推荐", 12, 0),
            "每日推荐 (12)"
        );
    }

    #[test]
    fn title_name_then_count() {
        assert_eq!(render_title("{name} ({count})", "歌单", 3, 0), "歌单 (3)");
    }

    #[test]
    fn title_no_placeholder() {
        assert_eq!(render_title("SONGS", "x", 0, 0), "SONGS");
    }

    #[test]
    fn title_adjacent_placeholders() {
        assert_eq!(render_title("{name}{count}", "A", 5, 0), "A5");
    }

    #[test]
    fn title_total_placeholder() {
        assert_eq!(
            render_title("{name} ({count}/{total})", "云盘", 50, 137),
            "云盘 (50/137)"
        );
    }
}
