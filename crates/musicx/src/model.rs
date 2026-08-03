use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Song {
    pub id: String,
    pub name: String,
    pub artists: Vec<Artist>,
    pub album: Option<Album>,
    pub duration: u64,
    pub source: MusicSource,
    pub quality: Option<Quality>,
    pub url: Option<String>,
    pub raw_data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Artist {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Album {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MusicSource {
    Kugou,
    Kuwo,
    BiliVideo,
    Youtube,
}

impl fmt::Display for MusicSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MusicSource::Kugou => write!(f, "kugou"),
            MusicSource::Kuwo => write!(f, "kuwo"),
            MusicSource::BiliVideo => write!(f, "bilivideo"),
            MusicSource::Youtube => write!(f, "youtube"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Quality {
    Standard,
    High,
    Lossless,
}

impl fmt::Display for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Quality::Standard => write!(f, "standard"),
            Quality::High => write!(f, "high"),
            Quality::Lossless => write!(f, "lossless"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchQuery {
    pub keyword: String,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    /// Optional expected duration in milliseconds, used as a scoring hint.
    pub duration: Option<u64>,
}

impl SearchQuery {
    pub fn new(keyword: impl Into<String>) -> Self {
        Self {
            keyword: keyword.into(),
            page: Some(1),
            page_size: Some(10),
            duration: None,
        }
    }

    pub fn with_page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    pub fn with_page_size(mut self, page_size: u32) -> Self {
        self.page_size = Some(page_size);
        self
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration = Some(duration_ms);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub songs: Vec<Song>,
    pub source: MusicSource,
    pub query: SearchQuery,
    pub total: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayUrlResult {
    pub url: String,
    pub quality: Quality,
    pub size: Option<u64>,
    pub bitrate: Option<u32>,
}
