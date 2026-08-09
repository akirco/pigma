use super::value_get;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- 歌手模型 ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingerInfo {
    pub id: u64,
    pub name: String,
    pub pic_url: String,
}

// --- 歌手解析 ---

pub(crate) fn parse_singer_info(value: &Value, path: &[&str]) -> Result<Vec<SingerInfo>, String> {
    let array = value_get(value, path)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("path {:?} not found", path))?;

    Ok(array
        .iter()
        .map(|v| SingerInfo {
            id: v["id"].as_u64().unwrap_or(0),
            name: v["name"].as_str().unwrap_or("unknown").to_string(),
            pic_url: {
                let url = v["img1v1Url"].as_str().unwrap_or("").to_string();
                if url.ends_with("5639395138885805.jpg") {
                    String::new()
                } else {
                    url
                }
            },
        })
        .collect())
}
