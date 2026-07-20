use crate::event::{AppEvent, CommandPanelAction};
use crate::state::App;
use crossterm::event::{KeyCode, KeyEvent, MouseEventKind};

pub(super) fn handle_command_key(app: &mut App, key_event: KeyEvent) {
    match key_event.code {
        KeyCode::Esc => {
            app.state
                .events
                .send(AppEvent::CommandPanel(CommandPanelAction::Close));
        }
        KeyCode::Up => {
            app.state
                .events
                .send(AppEvent::CommandPanel(CommandPanelAction::Previous));
        }
        KeyCode::Down => {
            app.state
                .events
                .send(AppEvent::CommandPanel(CommandPanelAction::Next));
        }
        KeyCode::Enter => {
            app.state
                .events
                .send(AppEvent::CommandPanel(CommandPanelAction::Select));
        }
        _ => {}
    }
}

pub(super) fn handle_command_mouse(app: &mut App, kind: MouseEventKind) {
    match kind {
        MouseEventKind::ScrollUp => {
            app.state
                .events
                .send(AppEvent::CommandPanel(CommandPanelAction::Previous));
        }
        MouseEventKind::ScrollDown => {
            app.state
                .events
                .send(AppEvent::CommandPanel(CommandPanelAction::Next));
        }
        _ => {}
    }
}
