mod command;
mod content;
mod help;
mod login;
mod main;
mod navigation;
mod search;
mod splash;
mod table;

use crate::app::App;
use crate::event::{AppEvent, CommandEvent, CommandPanelAction};
use crate::state::Page;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

pub fn handle_key_events(app: &mut App, key_event: KeyEvent) -> color_eyre::Result<()> {
    if key_event.modifiers == KeyModifiers::CONTROL {
        match key_event.code {
            KeyCode::Char('c' | 'C') => {
                app.state.events.send(AppEvent::Quit);
                return Ok(());
            }
            KeyCode::Char('p' | 'P') => {
                app.state
                    .events
                    .send(CommandEvent::Panel(CommandPanelAction::Open));
                return Ok(());
            }
            _ => {}
        }
    }

    if key_event.code == KeyCode::Char('?') {
        app.state.help.toggle();
        return Ok(());
    }

    if app.state.navigation.page == Page::Splash {
        splash::handle_splash_key(app, key_event);
        return Ok(());
    }

    if app.state.help.open {
        help::handle_help_key(app, key_event);
        return Ok(());
    }

    if app.state.command_panel.open {
        command::handle_command_key(app, key_event);
        return Ok(());
    }

    if app.state.navigation.page == Page::Login {
        login::handle_login_key(app, key_event);
        return Ok(());
    }

    if app.state.navigation.search.active && search::handle_search_key(app, key_event) {
        return Ok(());
    }

    if let KeyCode::Char(c) = key_event.code
        && c.eq_ignore_ascii_case(&'w')
        && key_event.modifiers == KeyModifiers::NONE
    {
        app.playback.clear_queue();
        app.toast("   已清空播放队列".into());
        if app.state.navigation.page == Page::Playlist {
            if let Some(key) = app.playback.switch_queue(false) {
                app.state.navigation.playlist_selected =
                    app.playback.queue_current_index().unwrap_or(0);
                app.toast(format!("▣ 队列: {key}"));
            } else if let Some(key) = app.playback.queue_keys().last().cloned() {
                // 清空后仅剩最后一个队列：switch_queue 在只剩一个时返回 None，
                // 这里显式聚焦它（取最右侧/最后一个标签），避免出现空焦点。
                app.playback.activate_queue(&key);
                app.state.navigation.playlist_selected =
                    app.playback.queue_current_index().unwrap_or(0);
                app.toast(format!("▣ 队列: {key}"));
            }
        }
        return Ok(());
    }

    main::handle_main_key(app, key_event)
}

pub fn handle_mouse_event(app: &mut App, kind: MouseEventKind, col: u16, row: u16) {
    if app.state.help.open {
        help::handle_help_mouse(app, kind);
        return;
    }

    if app.state.command_panel.open {
        command::handle_command_mouse(app, kind);
        return;
    }

    main::handle_main_mouse(app, kind, col, row);
}
