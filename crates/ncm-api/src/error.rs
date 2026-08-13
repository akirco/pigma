use serde_json::Value;
use thiserror::Error;

/// Extract the `code` from the raw NetEase Cloud response (default 0)
fn resp_code(v: &Value) -> i32 {
    v.get("code").and_then(|c| c.as_i64()).unwrap_or(0) as i32
}

/// Extract the error message from the raw NetEase Cloud response (msg / message)
fn resp_message(v: &Value) -> String {
    v.get("msg")
        .or_else(|| v.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("unknown error")
        .to_string()
}

#[derive(Debug, Error)]
pub enum NcmError {
    #[error("HTTP: {0}")]
    Http(#[from] reqwest::Error),

    /// The server returned a business error (non-200). The raw NetEase Cloud response is kept,
    /// and Display renders its code / msg directly so the real cause is easy to investigate.
    #[error("API code={}: {}", resp_code(.0), resp_message(.0))]
    Api(Value),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// The response succeeded but parsing the model failed. The raw response fragment and the
    /// parse error description are kept.
    #[error("parse: {message}\nresponse: {response}")]
    Parse { message: String, response: String },

    #[error("crypto: {0}")]
    Crypto(String),

    #[error("session: {0}")]
    Session(String),

    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
}

impl NcmError {
    /// Build a business error from a raw NetEase Cloud response
    pub fn api(value: Value) -> Self {
        Self::Api(value)
    }

    /// Build a parse error from a parse error message and the raw response that triggered it
    pub fn parse(message: impl Into<String>, response: &Value) -> Self {
        Self::Parse {
            message: message.into(),
            response: response.to_string(),
        }
    }
}
