use super::song::SongInfo;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- 发现 / 榜单模型 ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopList {
    pub id: u64,
    pub name: String,
    pub update: String,
    pub description: String,
    pub cover: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BannersInfo {
    pub pic: String,
    pub target_id: u64,
    pub target_type: TargetType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub songs: Vec<SongInfo>,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotSearchItem {
    pub keyword: String,
    pub icon_type: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TargetType {
    Song,
    Album,
    Unknown,
}

impl From<i32> for TargetType {
    fn from(t: i32) -> Self {
        match t {
            1 => Self::Song,
            10 => Self::Album,
            _ => Self::Unknown,
        }
    }
}

// --- 发现 / 榜单解析 ---

pub(crate) fn parse_toplist(value: &Value) -> Result<Vec<TopList>, String> {
    let array = value["list"].as_array().ok_or("list not found")?;
    Ok(array
        .iter()
        .map(|v| TopList {
            id: v["id"].as_u64().unwrap_or(0),
            name: v["name"].as_str().unwrap_or("unknown").to_string(),
            update: v["updateFrequency"].as_str().unwrap_or("").to_string(),
            description: v["description"].as_str().unwrap_or("").to_string(),
            cover: v["coverImgUrl"].as_str().unwrap_or("").to_string(),
        })
        .collect())
}

pub(crate) fn parse_banners(value: &Value) -> Result<Vec<BannersInfo>, String> {
    let array = value["banners"].as_array().ok_or("banners not found")?;
    Ok(array
        .iter()
        .map(|v| BannersInfo {
            pic: v["imageUrl"].as_str().unwrap_or("").to_string(),
            target_id: v["targetId"].as_u64().unwrap_or(0),
            target_type: TargetType::from(v["targetType"].as_i64().unwrap_or(0) as i32),
        })
        .collect())
}

pub(crate) fn parse_hot_search(value: &Value) -> Result<Vec<HotSearchItem>, String> {
    let array = value["data"].as_array().ok_or("data not found")?;
    array
        .iter()
        .map(|v| {
            Ok(HotSearchItem {
                keyword: v["searchWord"].as_str().unwrap_or("").to_string(),
                icon_type: v["iconType"].as_i64().unwrap_or(0) as i32,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_toplist() {
        let v = json!({
            "list": [
                {
                    "id": 19723756,
                    "name": "飙升榜",
                    "updateFrequency": "每天更新",
                    "description": "desc",
                    "coverImgUrl": "http://cover.png"
                }
            ]
        });
        let lists = parse_toplist(&v).unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].id, 19723756);
        assert_eq!(lists[0].name, "飙升榜");
    }

    #[test]
    fn test_parse_banners() {
        let v = json!({
            "banners": [
                {"imageUrl": "http://banner.png", "targetId": 100, "targetType": 1},
                {"imageUrl": "http://banner2.png", "targetId": 200, "targetType": 10}
            ]
        });
        let banners = parse_banners(&v).unwrap();
        assert_eq!(banners.len(), 2);
        assert_eq!(banners[0].target_type, TargetType::Song);
        assert_eq!(banners[1].target_type, TargetType::Album);
    }

    #[test]
    fn test_target_type_from() {
        assert_eq!(TargetType::from(1), TargetType::Song);
        assert_eq!(TargetType::from(10), TargetType::Album);
        assert_eq!(TargetType::from(99), TargetType::Unknown);
    }

    #[test]
    fn test_parse_hot_search() {
        let v = json!({
            "data": [
                {"searchWord": "keyword1", "iconType": 1},
                {"searchWord": "keyword2", "iconType": 0}
            ]
        });
        let items = parse_hot_search(&v).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].keyword, "keyword1");
    }
}
