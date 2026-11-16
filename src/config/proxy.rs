//! HTTP proxy target selection.

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyTarget {
    /// Domestic default: only YouTube goes through the proxy; everything else connects directly.
    #[default]
    Normal,
    /// Overseas users: everything except YouTube goes through the proxy.
    Reversed,
    /// Everything goes through the proxy.
    Both,
}

impl ProxyTarget {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "normal" => Some(Self::Normal),
            "reversed" => Some(Self::Reversed),
            "both" => Some(Self::Both),
            _ => None,
        }
    }
}

/// Unknown targets fall back to `normal` instead of failing the whole config.
impl<'de> Deserialize<'de> for ProxyTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from_name(&raw).unwrap_or_default())
    }
}
