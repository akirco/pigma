use thiserror::Error;

#[derive(Debug, Error)]
pub enum NcmError {
    #[error("HTTP: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API code={code}: {message}")]
    Api { code: i32, message: String },

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("crypto: {0}")]
    Crypto(String),

    #[error("session: {0}")]
    Session(String),

    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
}
