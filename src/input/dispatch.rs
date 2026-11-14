//! Top-level input dispatch: global shortcuts first, then the per-view handlers.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

use super::{command, equalizer, help, login, main, search, splash};
use crate::{
    app::App,
    event::{AppEvent, CommandEvent, CommandPanelAction, NavigationEvent},
    state::Page,
};

pub fn handle_key_events(app: &mut App, key_event: KeyEvent) -> eyre::Result<()> {
    if key_event.modifiers == KeyModifiers::CONTROL {
        match key_event.code {
            KeyCode::Char('c' | 'C') => {
                app.events.send(AppEvent::Quit);
                return Ok(());
            }
            KeyCode::Char('p' | 'P') => {
                app.events
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

    if app.state.navigation.page == Page::Equalizer {
        equalizer::handle_equalizer_key(app, key_event);
        return Ok(());
    }

    if app.state.navigation.search.active && search::handle_search_key(app, key_event) {
        return Ok(());
    }

    // Uppercase L: go to the login page (lowercase `l` toggles lyrics on the main page).
    // No-op when already logged in.
    if key_event.code == KeyCode::Char('L') && !app.is_logged_in() {
        app.events.send(NavigationEvent::Navigate(Page::Login));
        return Ok(());
    }

    if let KeyCode::Char(c) = key_event.code
        && c.eq_ignore_ascii_case(&'w')
        && key_event.modifiers == KeyModifiers::NONE
    {
        app.clear_play_queue();
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
