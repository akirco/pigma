use ncm_api::SongInfo;

#[derive(Debug, Clone)]
pub struct PlaylistQueue {
    pub songs: Vec<SongInfo>,
    pub history: Vec<SongInfo>,
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

    pub fn from_songs(songs: Vec<SongInfo>, index: usize) -> Self {
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

    pub fn current_song(&self) -> Option<&SongInfo> {
        self.current_index.and_then(|i| self.songs.get(i))
    }

    pub fn set_current(&mut self, index: usize) -> Option<&SongInfo> {
        if index < self.songs.len() {
            self.current_index = Some(index);
            self.songs.get(index)
        } else {
            None
        }
    }

    pub fn push_to_history(&mut self) {
        if let Some(i) = self.current_index
            && let Some(song) = self.songs.get(i)
        {
            self.history.push(song.clone());
        }
    }

    pub fn pop_history(&mut self) -> Option<SongInfo> {
        self.history.pop()
    }

    pub fn append(&mut self, songs: &[SongInfo]) -> usize {
        let offset = self.songs.len();
        self.songs.extend_from_slice(songs);
        offset
    }

    pub fn remove(&mut self, index: usize) -> Option<SongInfo> {
        if index >= self.songs.len() {
            return None;
        }
        let removed = self.songs.remove(index);
        self.current_index = match self.current_index {
            Some(i) if i == index && self.songs.is_empty() => None,
            Some(i) if i == index => Some(i.min(self.songs.len() - 1)),
            Some(i) if i > index => Some(i - 1),
            other => other,
        };
        Some(removed)
    }

    pub fn clear(&mut self) {
        self.songs.clear();
        self.current_index = None;
    }

    pub fn find_song_index(&self, song_id: u64) -> Option<usize> {
        self.songs.iter().position(|s| s.id == song_id)
    }
}
