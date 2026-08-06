use thiserror::Error;

#[derive(Error, Debug)]
pub enum SonarError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("Header error: {0}")]
    Header(#[from] reqwest::header::InvalidHeaderValue),

    #[error("Base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("Provider error: {provider} - {message}")]
    Provider { provider: String, message: String },

    #[error("No results found")]
    NoResults,

    #[error("Play URL not available")]
    NoPlayUrl,

    #[error("Lyrics not available")]
    NoLyrics,

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("WBI sign error: {0}")]
    WbiSign(String),

    #[error("Timeout")]
    Timeout,

    #[error("Rate limited")]
    RateLimited,

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

pub type Result<T> = std::result::Result<T, SonarError>;
