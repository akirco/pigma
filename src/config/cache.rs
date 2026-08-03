use serde::{Deserialize, Serialize};

/// 缓存配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// Content cache TTL in seconds (0 to disable).
    pub content_cache_ttl: u64,
    /// 边听边存缓存目录（绝对路径或相对于 ~/.cache/pigma/ 的路径）。
    pub cache_dir: String,
    /// 缓存文件命名模板。变量：{id} {name} {singer} {album}。
    /// 例："{name}-{singer}"
    pub cache_template: String,
    /// 边听边存音质等级：standard / higher / exhigh / lossless / hires。
    pub quality: String,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            content_cache_ttl: 300,
            cache_dir: "downloads".into(),
            cache_template: "{name}-{singer}".into(),
            quality: "standard".into(),
        }
    }
}
