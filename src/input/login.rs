use crate::event::{AppEvent, CommandPanelAction};
use crate::state::{App, LoginField, LoginMethod};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) fn handle_login_key(app: &mut App, key_event: KeyEvent) -> bool {
    if key_event.modifiers == KeyModifiers::CONTROL
        && matches!(key_event.code, KeyCode::Char('c' | 'C'))
    {
        app.state.events.send(AppEvent::Quit);
        return true;
    }
    if key_event.modifiers == KeyModifiers::CONTROL
        && matches!(key_event.code, KeyCode::Char('p' | 'P'))
    {
        app.state
            .events
            .send(AppEvent::CommandPanel(CommandPanelAction::Open));
        return true;
    }

    let login = &mut app.state.navigation.login;

    match key_event.code {
        KeyCode::Tab => {
            login.focus = match login.focus {
                LoginField::Method => LoginField::Username,
                LoginField::Username => LoginField::Password,
                LoginField::Password => LoginField::Method,
            };
        }
        KeyCode::BackTab => {
            login.focus = match login.focus {
                LoginField::Method => LoginField::Password,
                LoginField::Username => LoginField::Method,
                LoginField::Password => LoginField::Username,
            };
        }
        KeyCode::Left => {
            if login.focus == LoginField::Method {
                login.selected_method =
                    LoginMethod::from_index((login.selected_method.index() + 2) % 3);
            } else if login.focus == LoginField::Username {
                login.username.move_left();
            } else if login.focus == LoginField::Password {
                login.password.move_left();
            }
        }
        KeyCode::Right => {
            if login.focus == LoginField::Method {
                login.selected_method =
                    LoginMethod::from_index((login.selected_method.index() + 1) % 3);
            } else if login.focus == LoginField::Username {
                login.username.move_right();
            } else if login.focus == LoginField::Password {
                login.password.move_right();
            }
        }
        KeyCode::Char(c) => {
            if login.focus == LoginField::Username {
                login.username.enter_char(c);
            } else if login.focus == LoginField::Password {
                login.password.enter_char(c);
            }
        }
        KeyCode::Backspace => {
            if login.focus == LoginField::Username {
                login.username.delete_char();
            } else if login.focus == LoginField::Password {
                login.password.delete_char();
            }
        }
        KeyCode::Enter => {
            app.state.events.send(AppEvent::Login);
        }
        KeyCode::Esc => app.state.events.send(AppEvent::Quit),
        _ => {}
    }
    true
}
