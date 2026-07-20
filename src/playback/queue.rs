use std::sync::Arc;

use ncm_api::SongInfo;

use super::mode::PlayStrategy;

#[derive(Debug, Clone)]
pub struct PlaylistQueue {
    pub songs: Vec<Arc<SongInfo>>,
    pub history: Vec<Arc<SongInfo>>,
    pub current_index: Option<usize>,
}

impl PlaylistQueue {
    pub fn new() -> Self {
        Self {
            songs: Vec::new(),
            history: Vec::new(),
            current_index: None,
        }
    }

    pub fn from_songs(songs: Vec<Arc<SongInfo>>, index: usize) -> Self {
        Self {
            songs,
            history: Vec::new(),
            current_index: Some(index),
        }
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
            self.history.push(Arc::clone(song));
        }
    }

    pub fn pop_history(&mut self) -> Option<Arc<SongInfo>> {
        self.history.pop()
    }

    pub fn append(&mut self, songs: &[SongInfo]) -> usize {
        let offset = self.songs.len();
        self.songs.extend(songs.iter().map(|s| Arc::new(s.clone())));
        offset
    }

    pub fn find_song_index(&self, song_id: u64) -> Option<usize> {
        self.songs.iter().position(|s| s.id == song_id)
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
