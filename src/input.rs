mod command;
mod content;
mod login;
mod main;
mod navigation;
mod search;
mod splash;
mod table;

use crate::event::{AppEvent, CommandEvent, CommandPanelAction};
use crate::state::{App, Page};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

pub fn handle_key_events(app: &mut App, key_event: KeyEvent) -> color_eyre::Result<()> {
    // Global keys — bypass all page dispatch
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
            KeyCode::Char('l' | 'L') => {
                app.playback.clear_queue();
                app.toast("🗑  已清空播放队列".into());
                return Ok(());
            }
            _ => {}
        }
    }

    if app.state.navigation.page == Page::Splash {
        splash::handle_splash_key(app, key_event);
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

    main::handle_main_key(app, key_event)
}

pub fn handle_mouse_event(app: &mut App, kind: MouseEventKind) {
    if app.state.command_panel.open {
        command::handle_command_mouse(app, kind);
        return;
    }

    main::handle_main_mouse(app, kind);
}
