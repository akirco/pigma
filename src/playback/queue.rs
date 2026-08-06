use std::collections::HashMap;
use std::sync::Arc;

use ncm_api::SongInfo;

use super::mode::PlayStrategy;

const MAX_HISTORY: usize = 200;

#[derive(Debug, Clone)]
pub struct PlaylistQueue {
    pub songs: Vec<Arc<SongInfo>>,
    id_index: HashMap<u64, usize>,
    pub history: Vec<u64>,
    pub current_index: Option<usize>,
}

impl PlaylistQueue {
    pub fn new() -> Self {
        Self {
            songs: Vec::new(),
            id_index: HashMap::new(),
            history: Vec::new(),
            current_index: None,
        }
    }

    pub fn from_songs(songs: Vec<Arc<SongInfo>>, index: usize) -> Self {
        let mut q = Self {
            songs,
            id_index: HashMap::new(),
            history: Vec::new(),
            current_index: Some(index),
        };
        q.rebuild_index();
        q
    }

    /// Reconstruct the id→index lookup from the current `songs`. Cheap enough to
    /// call after any structural change; `or_insert` keeps the first (lowest)
    /// index for duplicate ids, matching the previous linear `position` scan.
    pub(crate) fn rebuild_index(&mut self) {
        self.id_index.clear();
        for (i, s) in self.songs.iter().enumerate() {
            self.id_index.entry(s.id).or_insert(i);
        }
    }

    /// Build a queue from restored parts, rebuilding the index afterwards.
    pub fn from_parts(
        songs: Vec<Arc<SongInfo>>,
        history: Vec<u64>,
        current_index: Option<usize>,
    ) -> Self {
        let mut q = Self {
            songs,
            id_index: HashMap::new(),
            history,
            current_index,
        };
        q.rebuild_index();
        q
    }

    /// Replace the song list and rebuild the id→index lookup.
    pub fn set_songs(&mut self, songs: Vec<Arc<SongInfo>>) {
        self.songs = songs;
        self.rebuild_index();
    }

    pub fn is_empty(&self) -> bool {
        self.songs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.songs.len()
    }

    pub fn current_song(&self) -> Option<&Arc<SongInfo>> {
        self.current_index.and_then(|i| self.songs.get(i))
    }

    pub fn push_to_history(&mut self) {
        if let Some(i) = self.current_index
            && let Some(song) = self.songs.get(i)
        {
            self.history.push(song.id);
            if self.history.len() > MAX_HISTORY {
                let drain = self.history.len() - MAX_HISTORY;
                self.history.drain(..drain);
            }
        }
    }

    pub fn pop_history(&mut self) -> Option<u64> {
        self.history.pop()
    }

    pub fn append(&mut self, songs: &[SongInfo]) -> usize {
        let offset = self.songs.len();
        self.songs.extend(songs.iter().map(|s| Arc::new(s.clone())));
        self.rebuild_index();
        offset
    }

    /// Insert `songs` right after the currently playing song so they play next.
    /// When nothing is playing, insert at the front. Returns the index of the
    /// first inserted song.
    pub fn insert_next(&mut self, songs: Vec<Arc<SongInfo>>) -> usize {
        let insert_at = self.current_index.map(|i| i + 1).unwrap_or(0);
        for (n, s) in songs.into_iter().enumerate() {
            self.songs.insert(insert_at + n, s);
        }
        self.rebuild_index();
        insert_at
    }

    pub fn find_song_index(&self, song_id: u64) -> Option<usize> {
        self.id_index.get(&song_id).copied()
    }

    pub fn next_index(&self, strategy: &mut dyn PlayStrategy) -> Option<usize> {
        strategy.next(self.current_index, self.songs.len())
    }

    pub fn prev_index(&self, strategy: &mut dyn PlayStrategy) -> Option<usize> {
        strategy.prev(self.current_index, self.songs.len())
    }

    pub fn advance_to(&mut self, index: usize) {
        if Some(index) != self.current_index {
            self.push_to_history();
        }
        self.current_index = Some(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: u64) -> Arc<SongInfo> {
        Arc::new(SongInfo {
            id,
            name: String::new(),
            singer: String::new(),
            artist_id: 0,
            album: String::new(),
            album_id: 0,
            pic_url: String::new(),
            duration: 0,
            copyright: ncm_api::SongCopyright::Unknown,
        })
    }

    /// `find_song_index` must agree with a linear `position` scan, including
    /// the first-match semantics for duplicate ids.
    #[test]
    fn find_song_index_matches_position() {
        let songs: Vec<Arc<SongInfo>> = vec![song(1), song(2), song(3), song(2)];
        let q = PlaylistQueue::from_songs(songs, 0);
        // duplicate id 2 -> first occurrence at index 1
        assert_eq!(q.find_song_index(2), Some(1));
        assert_eq!(q.find_song_index(3), Some(2));
        assert_eq!(q.find_song_index(9), None);
    }

    #[test]
    fn index_stays_correct_after_append() {
        let mut q = PlaylistQueue::from_songs(vec![song(1), song(2)], 0);
        q.append(&[song(3).as_ref().clone()]);
        assert_eq!(q.find_song_index(3), Some(2));
        assert_eq!(q.find_song_index(1), Some(0));
    }

    #[test]
    fn index_stays_correct_after_insert_next() {
        let mut q = PlaylistQueue::from_songs(vec![song(1), song(2), song(3)], 1);
        // insert after current (index 1): new songs land at 2, old 3 shifts to 3
        let at = q.insert_next(vec![song(9)]);
        assert_eq!(at, 2);
        assert_eq!(q.find_song_index(9), Some(2));
        assert_eq!(q.find_song_index(3), Some(3));
    }

    #[test]
    fn from_parts_rebuilds_index() {
        let q = PlaylistQueue::from_parts(vec![song(7), song(8)], vec![7], Some(0));
        assert_eq!(q.find_song_index(8), Some(1));
        assert_eq!(q.history, vec![7]);
    }
}
