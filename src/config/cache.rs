use ncm_api::SongQuality;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

fn default_quality() -> SongQuality {
    SongQuality::Standard
}

/// The config uses the same lower-case level strings as the API (`standard`,
/// `higher`, `exhigh`, ...), while `SongQuality`'s own serde derive uses
/// variant names. Keep the config representation stable with a field-local
/// serializer instead of redefining the enum.
fn serialize_quality<S>(quality: &SongQuality, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(quality.as_level())
}

/// Unknown quality strings fall back to `standard` instead of failing the
/// whole config parse.
fn deserialize_quality<'de, D>(deserializer: D) -> Result<SongQuality, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(SongQuality::from_level(&raw).unwrap_or_else(|| {
        log::warn!("Unknown cache quality `{raw}`, using standard");
        SongQuality::Standard
    }))
}

/// Cache configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// Content cache TTL in seconds (0 to disable).
    pub content_cache_ttl: u64,
    /// Save-on-play cache directory (absolute path or a path relative to ~/.cache/pigma/).
    pub cache_dir: String,
    /// Cache file naming template. Variables: {id} {name} {singer} {album}.
    /// Example: "{name}-{singer}"
    pub cache_template: String,
    /// Save-on-play quality level: standard / higher / exhigh / lossless / hires / jyeffect / sky / jymaster.
    #[serde(
        default = "default_quality",
        serialize_with = "serialize_quality",
        deserialize_with = "deserialize_quality"
    )]
    pub quality: SongQuality,
    /// Save-on-play: automatically write to the download cache while playing. Set to false to stream only, without writing to disk.
    #[serde(default = "default_save_on_play")]
    pub save_on_play: bool,
}

fn default_save_on_play() -> bool {
    true
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            content_cache_ttl: 300,
            cache_dir: "downloads".into(),
            cache_template: "{name}-{singer}".into(),
            quality: SongQuality::Standard,
            save_on_play: default_save_on_play(),
        }
    }
}
