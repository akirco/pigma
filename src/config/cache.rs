use serde::{Deserialize, Serialize};

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
    /// Save-on-play quality level: standard / higher / exhigh / lossless / hires.
    pub quality: String,
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
            quality: "standard".into(),
            save_on_play: default_save_on_play(),
        }
    }
}
