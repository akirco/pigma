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
pub mod text_input;
mod topbar;

use std::sync::OnceLock;

use ratatui::{
    Frame,
    style::Style,
    widgets::{
        Block, BorderType, Borders, Padding, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
};

use self::block::CornerBlock;

use crate::{
    layout,
    state::{App, Page},
    config::Theme,
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
                for _ in 0..5 {
                    chars.next();
                }
            } else if template[i..].starts_with("{count}") {
                result.push_str(&count.to_string());
                for _ in 0..6 {
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

fn theme_fallback() -> &'static Theme {
    static FALLBACK: OnceLock<Theme> = OnceLock::new();
    FALLBACK.get_or_init(Theme::default)
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let now = std::time::Instant::now();
    let steps = (now.duration_since(app.state.last_tick).as_millis() / 80).max(1) as u64;
    app.state.last_tick = now;
    app.state.tick = app.state.tick.wrapping_add(steps);

    let area = f.area();
    let bordered = app.state.bordered;
    let border_rounded = app.state.border_rounded;

    let colors = app
        .theme_registry
        .get(&app.state.current_color_name)
        .or_else(|| app.theme_registry.get("default"))
        .unwrap_or_else(|| {
            log::error!("No theme found, using fallback");
            theme_fallback()
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
                        format!(" {} ", render_title(template, name, count))
                    };
                    let block = create_block(&title, colors, bordered, border_rounded, false);
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
}

pub(crate) fn create_block<'a>(
    title: &'a str,
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
            .title(title.to_string())
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
            .title(format!(" {} ", title))
            .title_style(Style::default().fg(border_color))
            .padding(Padding::horizontal(1))
    };
    let corner = colors.accent;
    CornerBlock::new(block)
        .corner_color(corner)
        .corner_sizes(2, 1)
}
