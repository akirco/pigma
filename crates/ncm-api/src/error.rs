use serde_json::Value;
use thiserror::Error;

/// 提取网易云原始响应里的 code（默认 0）
fn resp_code(v: &Value) -> i32 {
    v.get("code").and_then(|c| c.as_i64()).unwrap_or(0) as i32
}

/// 提取网易云原始响应里的错误信息（msg / message）
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

    /// 服务端返回了业务错误（非 200）。保留网易云原始响应，
    /// Display 直接渲染其中的 code / msg，便于排查真实原因。
    #[error("API code={}: {}", resp_code(.0), resp_message(.0))]
    Api(Value),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// 响应成功但解析模型失败。保留原始响应片段与解析错误说明。
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
    /// 由网易云原始响应构造业务错误
    pub fn api(value: Value) -> Self {
        Self::Api(value)
    }

    /// 由解析错误与触发它的原始响应构造解析错误
    pub fn parse(message: impl Into<String>, response: &Value) -> Self {
        Self::Parse {
            message: message.into(),
            response: response.to_string(),
        }
    }
}
