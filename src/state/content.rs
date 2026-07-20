use serde::{Deserialize, Serialize};

use ncm_api::{SingerInfo, SongInfo, SongList, TopList};

use crate::config::ContentType;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ContentState {
    Empty,
    Loading,
    Error(String),
    Songs(Vec<SongInfo>),
    SongLists(Vec<SongList>),
    TopLists(Vec<TopList>),
    HotSearch(Vec<String>),
    Singers(Vec<SingerInfo>),
}

impl ContentState {
    pub fn len(&self) -> usize {
        match self {
            ContentState::Songs(s) => s.len(),
            ContentState::SongLists(l) => l.len(),
            ContentState::TopLists(l) => l.len(),
            ContentState::HotSearch(kw) => kw.len(),
            ContentState::Singers(s) => s.len(),
            _ => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn content_type(&self) -> ContentType {
        match self {
            ContentState::Songs(_) => ContentType::Songs,
            ContentState::SongLists(_) => ContentType::SongLists,
            ContentState::TopLists(_) => ContentType::TopLists,
            ContentState::HotSearch(_) => ContentType::HotSearch,
            ContentState::Singers(_) => ContentType::Singers,
            ContentState::Empty | ContentState::Loading | ContentState::Error(_) => {
                ContentType::Empty
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableMode {
    Row,
    Cell,
}
