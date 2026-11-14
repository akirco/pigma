//! HTTP proxy target selection.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyTarget {
    /// Domestic default: only YouTube goes through the proxy; everything else connects directly.
    Normal,
    /// Overseas users: everything except YouTube goes through the proxy.
    Reversed,
    /// Everything goes through the proxy.
    Both,
}
