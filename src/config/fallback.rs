//! Sonar fallback-source configuration.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sonar::SonarSource;

fn provider_name(source: SonarSource) -> &'static str {
    match source {
        SonarSource::Kuwo => "kuwo",
        SonarSource::Kugou => "kugou",
        SonarSource::BiliVideo => "bilivideo",
        SonarSource::Youtube => "youtube",
    }
}

fn provider_from_name(name: &str) -> Option<SonarSource> {
    match name {
        "kuwo" => Some(SonarSource::Kuwo),
        "kugou" => Some(SonarSource::Kugou),
        "bilivideo" => Some(SonarSource::BiliVideo),
        "youtube" => Some(SonarSource::Youtube),
        _ => None,
    }
}

fn serialize_providers<S>(providers: &[SonarSource], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.collect_seq(providers.iter().map(|source| provider_name(*source)))
}

/// Ignore unknown provider names (with a warning) instead of failing the
/// whole config; duplicates are de-duplicated by the finder builder.
fn deserialize_providers<'de, D>(deserializer: D) -> Result<Vec<SonarSource>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Vec::<String>::deserialize(deserializer)?;
    Ok(raw
        .iter()
        .filter_map(|name| {
            let provider = provider_from_name(name);
            if provider.is_none() {
                log::warn!("Unknown fallback provider `{name}`, ignoring it");
            }
            provider
        })
        .collect())
}

/// Fallback source config (sonar multi-source fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SonarConfig {
    /// Whether fallback sources are enabled.
    pub enabled: bool,
    /// Sources participating in fallback, ordered from highest to lowest priority:
    /// `kuwo`, `kugou`, `bilivideo`, `youtube`.
    #[serde(
        default,
        serialize_with = "serialize_providers",
        deserialize_with = "deserialize_providers"
    )]
    pub providers: Vec<SonarSource>,
    /// Per-source search timeout (milliseconds).
    pub timeout_ms: u64,
}

impl Default for SonarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            providers: vec![
                SonarSource::Kuwo,
                SonarSource::Kugou,
                SonarSource::BiliVideo,
                SonarSource::Youtube,
            ],
            timeout_ms: 10000,
        }
    }
}
