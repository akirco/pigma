use crossterm::event::{KeyCode, KeyEvent, MouseEventKind};

use crate::{
    app::App,
    event::{CommandEvent, CommandPanelAction},
};

pub(super) fn handle_command_key(app: &mut App, key_event: KeyEvent) {
    match key_event.code {
        KeyCode::Esc => {
            app.events
                .send(CommandEvent::Panel(CommandPanelAction::Close));
        }
        KeyCode::Up => {
            app.events
                .send(CommandEvent::Panel(CommandPanelAction::Previous));
        }
        KeyCode::Down => {
            app.events
                .send(CommandEvent::Panel(CommandPanelAction::Next));
        }
        KeyCode::Enter => {
            app.events
                .send(CommandEvent::Panel(CommandPanelAction::Select));
        }
        _ => {}
    }
}

pub(super) fn handle_command_mouse(app: &mut App, kind: MouseEventKind) {
    match kind {
        MouseEventKind::ScrollUp => {
            app.events
                .send(CommandEvent::Panel(CommandPanelAction::Previous));
        }
        MouseEventKind::ScrollDown => {
            app.events
                .send(CommandEvent::Panel(CommandPanelAction::Next));
        }
        _ => {}
    }
}
