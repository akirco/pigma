use std::path::PathBuf;

/// 把文件名中的非法字符替换为 `_`。
pub fn sanitize_filename(s: &str) -> String {
    s.trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// pigma 缓存根目录（缓存文件、播放队列均在此之下）。
pub fn pigma_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pigma")
}

/// pigma 配置根目录。
pub fn pigma_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pigma")
}
