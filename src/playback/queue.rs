use std::{collections::HashMap, sync::Arc};

use ncm_api::SongInfo;

use super::mode::PlayStrategy;

const MAX_HISTORY: usize = 200;

#[derive(Debug, Clone)]
pub struct PlaylistQueue {
    pub songs: Vec<Arc<SongInfo>>,
    id_index: HashMap<u64, usize>,
    pub history: Vec<u64>,
    pub current_index: Option<usize>,
    /// Monotonic counter bumped on every queue mutation. Consumers (e.g. the IPC
    /// queue snapshot) use it to rebuild only when the queue actually changes
    /// instead of cloning the whole list every frame.
    version: u64,
}

impl PlaylistQueue {
    pub(super) fn new() -> Self {
        Self {
            songs: Vec::new(),
            id_index: HashMap::new(),
            history: Vec::new(),
            current_index: None,
            version: 0,
        }
    }

    pub(super) fn version(&self) -> u64 {
        self.version
    }

    /// Replace this queue's contents with `next`, carrying the change counter
    /// forward so consumers watching [`Self::version`] (e.g. the IPC queue
    /// snapshot) still detect the swap. A freshly built queue starts at 0, which
    /// could otherwise collide with this queue's value and hide the change.
    pub(super) fn replace_with(&mut self, mut next: PlaylistQueue) {
        next.version = self.version.wrapping_add(1);
        *self = next;
    }

    /// Mark the queue as changed. Used when callers mutate `current_index`
    /// directly (outside the methods that bump automatically).
    pub(super) fn bump(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    pub(super) fn from_songs(songs: Vec<Arc<SongInfo>>, index: usize) -> Self {
        let mut q = Self {
            songs,
            id_index: HashMap::new(),
            history: Vec::new(),
            current_index: Some(index),
            version: 0,
        };
        q.rebuild_index();
        q
    }

    /// Reconstruct the id→index lookup from the current `songs`. Cheap enough to
    /// call after any structural change; `or_insert` keeps the first (lowest)
    /// index for duplicate ids, matching the previous linear `position` scan.
    pub(super) fn rebuild_index(&mut self) {
        self.id_index.clear();
        for (i, s) in self.songs.iter().enumerate() {
            self.id_index.entry(s.id).or_insert(i);
        }
    }

    /// Build a queue from restored parts, rebuilding the index afterwards.
    pub(super) fn from_parts(
        songs: Vec<Arc<SongInfo>>,
        history: Vec<u64>,
        current_index: Option<usize>,
    ) -> Self {
        let mut q = Self {
            songs,
            id_index: HashMap::new(),
            history,
            current_index,
            version: 0,
        };
        q.rebuild_index();
        q
    }

    /// Replace the song list and rebuild the id→index lookup.
    ///
    /// `keep_current` is the id of the song whose playhead should survive the
    /// swap (queue filter/unfilter). The playhead only stays set while that
    /// song is part of the new list: a filtered-out song keeps playing, but
    /// the queue itself has no current entry to point at.
    pub(super) fn set_songs(&mut self, songs: Vec<Arc<SongInfo>>, keep_current: Option<u64>) {
        self.songs = songs;
        self.rebuild_index();
        self.current_index = keep_current.and_then(|id| self.find_song_index(id));
        self.bump();
    }

    pub(super) fn is_empty(&self) -> bool {
        self.songs.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.songs.len()
    }

    pub(super) fn current_song(&self) -> Option<&Arc<SongInfo>> {
        self.current_index.and_then(|i| self.songs.get(i))
    }

    pub(super) fn push_to_history(&mut self) {
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

    pub(super) fn pop_history(&mut self) -> Option<u64> {
        self.history.pop()
    }

    pub(super) fn append(&mut self, songs: &[Arc<SongInfo>]) -> usize {
        let offset = self.songs.len();
        self.songs.extend(
            songs
                .iter()
                .filter(|s| !self.id_index.contains_key(&s.id))
                .map(Arc::clone),
        );
        self.rebuild_index();
        self.bump();
        offset
    }

    /// Insert `songs` right after the currently playing song so they play next.
    /// When nothing is playing, insert at the front. Returns the index of the
    /// first inserted song.
    pub(super) fn insert_next(&mut self, songs: Vec<Arc<SongInfo>>) -> usize {
        let insert_at = self.current_index.map(|i| i + 1).unwrap_or(0);
        for (n, s) in songs.into_iter().enumerate() {
            self.songs.insert(insert_at + n, s);
        }
        self.rebuild_index();
        self.bump();
        insert_at
    }

    pub(super) fn find_song_index(&self, song_id: u64) -> Option<usize> {
        self.id_index.get(&song_id).copied()
    }

    pub(super) fn next_index(&self, strategy: &mut dyn PlayStrategy) -> Option<usize> {
        strategy.next(self.current_index, self.songs.len())
    }

    pub(super) fn prev_index(&self, strategy: &mut dyn PlayStrategy) -> Option<usize> {
        strategy.prev(self.current_index, self.songs.len())
    }

    pub(super) fn advance_to(&mut self, index: usize) {
        if Some(index) != self.current_index {
            self.push_to_history();
        }
        self.current_index = Some(index);
        self.bump();
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
        q.append(&[song(3)]);
        assert_eq!(q.find_song_index(3), Some(2));
        assert_eq!(q.find_song_index(1), Some(0));
    }

    /// Filtering the queue (and widening it again) must keep the playhead on
    /// the same song while it is present, and unset it while filtered out —
    /// never leave a stale/out-of-range index behind.
    #[test]
    fn set_songs_follows_the_current_song_by_id() {
        let mut q = PlaylistQueue::from_songs(vec![song(1), song(2), song(3)], 1);

        // Filter out the playing song (2): the playhead is unset, not left at 1.
        q.set_songs(vec![song(3), song(1)], Some(2));
        assert_eq!(q.current_index, None);
        assert_eq!(q.find_song_index(1), Some(1));

        // Widen again: the playhead follows the same song to its new index.
        q.set_songs(vec![song(1), song(2), song(3)], Some(2));
        assert_eq!(q.current_index, Some(1));
        assert_eq!(q.current_song().map(|s| s.id), Some(2));
        assert_eq!(q.find_song_index(3), Some(2));
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

    /// A replacement queue must not reset the change counter: the IPC queue
    /// snapshot keys off `version`, so a reset could collide with the previous
    /// value and hide the queue switch (`pigma status -L` showing a stale queue).
    #[test]
    fn replace_with_keeps_version_monotonic() {
        let mut q = PlaylistQueue::from_songs(vec![song(1)], 0);
        q.bump();
        assert_eq!(q.version(), 1);

        let replacement = PlaylistQueue::from_songs(vec![song(2)], 0);
        assert_eq!(replacement.version(), 0);
        q.replace_with(replacement);

        assert_eq!(q.version(), 2);
        assert_eq!(q.songs[0].id, 2);
        assert_eq!(q.current_song().map(|s| s.id), Some(2));
    }
}
