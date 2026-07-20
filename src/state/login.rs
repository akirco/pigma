use crate::text_input::TextInput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginMethod {
    QR,
    Phone,
    Email,
}

impl LoginMethod {
    pub fn index(&self) -> usize {
        match self {
            LoginMethod::QR => 0,
            LoginMethod::Phone => 1,
            LoginMethod::Email => 2,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i {
            0 => LoginMethod::QR,
            1 => LoginMethod::Phone,
            _ => LoginMethod::Email,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginField {
    Username,
    Password,
    Method,
}

pub struct LoginState {
    pub selected_method: LoginMethod,
    pub username: TextInput,
    pub password: TextInput,
    pub focus: LoginField,
    pub loading: bool,
    pub error: Option<String>,
    pub captcha_sent: bool,
    pub qr_url: String,
    pub qr_key: String,
    pub qr_status_text: String,
}

impl Default for LoginState {
    fn default() -> Self {
        Self {
            selected_method: LoginMethod::Email,
            username: TextInput::new(),
            password: TextInput::new(),
            focus: LoginField::Method,
            loading: false,
            error: None,
            captcha_sent: false,
            qr_url: String::new(),
            qr_key: String::new(),
            qr_status_text: String::new(),
        }
    }
}
