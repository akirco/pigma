use std::sync::Arc;

use ncm_api::{SingerInfo, SongInfo, SongList, TopList};
use serde::{Deserialize, Serialize};

use crate::config::ContentType;

/// Newtype for hot-search keyword list, providing type safety over bare `Vec<String>`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HotSearchKeywords(pub Vec<String>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ContentState {
    Empty,
    Loading,
    Error(String),
    Songs(Vec<Arc<SongInfo>>),
    SongLists(Vec<SongList>),
    TopLists(Vec<TopList>),
    HotSearch(HotSearchKeywords),
    Singers(Vec<SingerInfo>),
}

impl ContentState {
    pub fn len(&self) -> usize {
        match self {
            ContentState::Songs(s) => s.len(),
            ContentState::SongLists(l) => l.len(),
            ContentState::TopLists(l) => l.len(),
            ContentState::HotSearch(kw) => kw.0.len(),
            ContentState::Singers(s) => s.len(),
            _ => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Remove a song by ID from song content. Returns whether an item was removed.
    pub fn remove_song(&mut self, song_id: u64) -> bool {
        let ContentState::Songs(songs) = self else {
            return false;
        };
        let previous_len = songs.len();
        songs.retain(|song| song.id != song_id);
        songs.len() != previous_len
    }

    /// Append only songs whose IDs are not already present. Returns the number appended.
    pub fn append_unique_songs(&mut self, new_songs: Vec<Arc<SongInfo>>) -> usize {
        let ContentState::Songs(songs) = self else {
            return 0;
        };
        let mut ids: std::collections::HashSet<u64> = songs.iter().map(|song| song.id).collect();
        let previous_len = songs.len();
        songs.extend(new_songs.into_iter().filter(|song| ids.insert(song.id)));
        songs.len() - previous_len
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

#[cfg(test)]
mod tests {
    use super::*;
    use ncm_api::SongCopyright;

    fn song(id: u64) -> Arc<SongInfo> {
        Arc::new(SongInfo {
            id,
            name: format!("song-{id}"),
            singer: String::new(),
            artist_id: 0,
            album: String::new(),
            album_id: 0,
            pic_url: String::new(),
            duration: 0,
            copyright: SongCopyright::Free,
        })
    }

    #[test]
    fn remove_song_removes_only_the_matching_id() {
        let mut content = ContentState::Songs(vec![song(1), song(2), song(3)]);

        assert!(content.remove_song(2));
        let ContentState::Songs(songs) = content else {
            panic!("expected song content");
        };
        assert_eq!(
            songs.iter().map(|song| song.id).collect::<Vec<_>>(),
            vec![1, 3]
        );
    }

    #[test]
    fn remove_song_ignores_non_song_content_and_missing_ids() {
        let mut empty = ContentState::Empty;
        let mut songs = ContentState::Songs(vec![song(1)]);

        assert!(!empty.remove_song(1));
        assert!(!songs.remove_song(2));
        assert_eq!(songs.len(), 1);
    }

    #[test]
    fn append_unique_songs_skips_existing_and_incoming_duplicates() {
        let mut content = ContentState::Songs(vec![song(1), song(2)]);

        assert_eq!(
            content.append_unique_songs(vec![song(2), song(3), song(3), song(4)]),
            2
        );
        let ContentState::Songs(songs) = content else {
            panic!("expected song content");
        };
        assert_eq!(
            songs.iter().map(|song| song.id).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
    }
}
