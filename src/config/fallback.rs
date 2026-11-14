//! Sonar fallback-source configuration.

use serde::{Deserialize, Serialize};

/// Fallback source config (sonar multi-source fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SonarConfig {
    /// Whether fallback sources are enabled.
    pub enabled: bool,
    /// Sources participating in fallback, ordered from highest to lowest priority:
    /// `kuwo`, `kugou`, `bilivideo`, `youtube`.
    pub providers: Vec<String>,
    /// Per-source search timeout (milliseconds).
    pub timeout_ms: u64,
}

impl Default for SonarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            providers: vec![
                "kuwo".to_string(),
                "kugou".to_string(),
                "bilivideo".to_string(),
                "youtube".to_string(),
            ],
            timeout_ms: 10000,
        }
    }
}
