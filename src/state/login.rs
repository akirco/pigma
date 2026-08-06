#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoginState {
    pub loading: bool,
    pub error: Option<String>,
    pub qr_url: String,
    pub qr_key: String,
    pub qr_status_text: String,
}
