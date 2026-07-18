use std::collections::HashMap;

use ncm_api::{SingerInfo, SongInfo, SongList, TopList};

/// Trait for converting a data model into a field→string map for table rendering.
pub trait ToFieldMap {
    fn to_field_map(&self) -> HashMap<String, String>;
}

impl ToFieldMap for SongInfo {
    fn to_field_map(&self) -> HashMap<String, String> {
        let mut m = HashMap::with_capacity(4);
        m.insert("name".into(), self.name.clone());
        m.insert("singer".into(), self.singer.clone());
        m.insert("album".into(), self.album.clone());
        m.insert("duration".into(), crate::utils::format_duration(self.duration));
        m
    }
}

impl ToFieldMap for SongList {
    fn to_field_map(&self) -> HashMap<String, String> {
        let mut m = HashMap::with_capacity(2);
        m.insert("name".into(), self.name.clone());
        m.insert("author".into(), self.author.clone());
        m
    }
}

impl ToFieldMap for TopList {
    fn to_field_map(&self) -> HashMap<String, String> {
        let mut m = HashMap::with_capacity(2);
        m.insert("name".into(), self.name.clone());
        m.insert("description".into(), self.description.clone());
        m
    }
}

impl ToFieldMap for SingerInfo {
    fn to_field_map(&self) -> HashMap<String, String> {
        let mut m = HashMap::with_capacity(2);
        m.insert("name".into(), self.name.clone());
        m.insert("id".into(), self.id.to_string());
        m
    }
}

/// Generic fallback using serde_json for any other Serialize type.
pub fn to_map<T: serde::Serialize>(item: &T) -> HashMap<String, String> {
    use serde_json::Value;

    let value = match serde_json::to_value(item) {
        Ok(Value::Object(map)) => map,
        _ => return HashMap::new(),
    };

    value
        .into_iter()
        .map(|(k, v)| {
            let s = match v {
                Value::String(s) => s,
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null | Value::Array(_) | Value::Object(_) => String::new(),
            };
            (k, s)
        })
        .collect()
}
