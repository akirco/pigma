mod block;
mod breadcrumb;
mod command_panel;
mod content;
mod login;
mod lyrics;
mod navigation;
mod playerbar;
mod queue;
mod spinner;
mod splash;
pub mod styled_text;
pub mod table;
mod topbar;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    widgets::{
        Block, BorderType, Borders, Clear, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
};
use std::time::Duration;

use self::block::CornerBlock;

use crate::{
    config::Theme,
    layout,
    state::{App, Page},
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

pub fn render_scrollbar(f: &mut Frame, total: usize, selected: usize, area: ratatui::layout::Rect) {
    let mut state = ScrollbarState::new(total).position(selected);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .thumb_symbol("│")
        .track_symbol(None);
    f.render_stateful_widget(scrollbar, area, &mut state);
}

/// Render a title template with `{name}` and `{count}` placeholders.
pub fn render_title<'a>(template: &'a str, name: &str, count: usize) -> std::borrow::Cow<'a, str> {
    if !template.contains('{') {
        return std::borrow::Cow::Borrowed(template);
    }
    let mut result = String::with_capacity(template.len() + name.len() + 8);
    let mut chars = template.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch == '{' {
            if template[i..].starts_with("{name}") {
                result.push_str(name);
                for _ in 0..("{name}".len() - 1) {
                    chars.next();
                }
            } else if template[i..].starts_with("{count}") {
                use std::fmt::Write;
                let _ = write!(result, "{count}");
                for _ in 0..("{count}".len() - 1) {
                    chars.next();
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }
    std::borrow::Cow::Owned(result)
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
    let bordered = app.state.bordered;
    let border_rounded = app.state.border_rounded;

    let colors = app
        .theme_registry
        .get(&app.config.default_theme)
        .or_else(|| app.theme_registry.get("default"))
        .unwrap_or_else(|| {
            log::error!("No theme found, using fallback");
            crate::state::theme_fallback()
        });

    match app.state.navigation.page {
        Page::Splash => {
            let lay = layout::splash(area);
            splash::draw(f, &app.state.splash, colors, &lay);
        }
        Page::Login => {
            let lay = layout::login(area);
            login::draw(f, &app.state.navigation.login, colors, bordered, &lay);
        }
        page => {
            let lay = layout::build_layout(area, page);

            topbar::draw(
                f,
                app.state.navigation.user.as_ref(),
                &app.state.navigation.search,
                colors,
                bordered,
                border_rounded,
                lay.topbar,
            );
            playerbar::draw(
                f,
                &app.playback.state,
                app.state.tick,
                colors,
                bordered,
                border_rounded,
                &app.config.playerbar,
                lay.playerbar,
            );

            match page {
                Page::Main => {
                    navigation::draw(
                        f,
                        &mut app.state.navigation.nav,
                        colors,
                        bordered,
                        border_rounded,
                        &app.config.titles.sidebar,
                        lay.sidebar,
                    );

                    breadcrumb::render_breadcrumb(
                        f,
                        &app.state.navigation.nav,
                        colors,
                        bordered,
                        border_rounded,
                        lay.breadcrumb,
                    );

                    let nav = &app.state.navigation.nav;
                    let current_item = nav
                        .section_states
                        .get(nav.focus_section)
                        .and_then(|st| st.selected())
                        .and_then(|i| nav.sections.get(nav.focus_section)?.items.get(i));

                    let title = {
                        let name = current_item
                            .map(|item| item.name.as_str())
                            .unwrap_or("SONGS");
                        let count = app.state.navigation.content.len();
                        let template = current_item
                            .and_then(|item| item.title_template.as_deref())
                            .unwrap_or("{name} ({count})");
                        render_title(template, name, count)
                    };
                    let block = create_block(title, colors, bordered, border_rounded, false);
                    let inner = block.inner(lay.content);
                    f.render_widget(block, lay.content);

                    let api = current_item.and_then(|item| item.api.as_deref());

                    content::render_content(
                        f,
                        &app.state.navigation.content,
                        &app.config.columns,
                        api,
                        &app.state.navigation.content_rows_cache,
                        colors,
                        &mut app.state.navigation.table_state,
                        app.state.navigation.table_mode,
                        inner,
                    );
                }
                Page::Lyrics => {
                    lyrics::draw(
                        f,
                        &app.playback.state,
                        colors,
                        bordered,
                        border_rounded,
                        &app.config.lyric_gradient,
                        &app.config.titles.lyrics,
                        lay.content,
                    );
                }
                Page::Playlist => {
                    queue::draw_queue_table(
                        f,
                        &app.playback,
                        app.state.navigation.playlist_selected,
                        colors,
                        bordered,
                        border_rounded,
                        &app.config.titles.playlist,
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
    let char_count = app.state.toast_msg.chars().count();
    let w = (char_count + 6).min(area.width as usize) as u16;
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
        .border_style(Style::default().fg(colors.accent))
        .style(Style::default().bg(colors.surface));

    let p = Paragraph::new(format!(" {} ", app.state.toast_msg))
        .style(Style::default().fg(colors.text))
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(p, toast_area);
}

pub(crate) fn create_block<'a>(
    title: impl Into<ratatui::text::Line<'a>>,
    colors: &'a Theme,
    bordered: bool,
    border_rounded: bool,
    focused: bool,
) -> CornerBlock<'a> {
    let border_color = if focused { colors.accent } else { colors.muted };
    let border_type = if border_rounded {
        BorderType::Rounded
    } else {
        BorderType::Plain
    };
    let block = if bordered {
        Block::default()
            .borders(Borders::ALL)
            .border_type(border_type)
            .border_style(Style::default().fg(border_color))
            .title(title)
            .title_style(Style::default().fg(border_color))
    } else {
        Block::default()
            .borders(Borders::NONE)
            .border_style(Style::default().fg(if focused {
                colors.accent
            } else {
                colors.surface
            }))
            .style(Style::default().bg(colors.bg))
            .title(title)
            .title_style(Style::default().fg(border_color))
            .padding(Padding::horizontal(1))
    };
    let corner = colors.accent;
    CornerBlock::new(block)
        .corner_color(corner)
        .corner_sizes(2, 1)
}

#[cfg(test)]
mod tests {
    use super::render_title;

    #[test]
    fn title_with_count_suffix() {
        assert_eq!(
            render_title("每日推荐 ({count})", "每日推荐", 12),
            "每日推荐 (12)"
        );
    }

    #[test]
    fn title_name_then_count() {
        assert_eq!(render_title("{name} ({count})", "歌单", 3), "歌单 (3)");
    }

    #[test]
    fn title_no_placeholder() {
        assert_eq!(render_title("SONGS", "x", 0), "SONGS");
    }

    #[test]
    fn title_adjacent_placeholders() {
        assert_eq!(render_title("{name}{count}", "A", 5), "A5");
    }
}
