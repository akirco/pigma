//! Playback engine events, kept with the playback module so lower layers do not depend on the app event hub.

use std::{sync::Arc, time::Duration};

use ncm_api::SongInfo;

use super::LyricLine;

#[derive(Clone, Debug)]
pub enum PlaybackEvent {
    SongPlay(u64),
    Started,
    Progress {
        position: Duration,
        total: Option<Duration>,
    },
    Finished,
    Error(String),
    /// Output-device failure (`Some`) or recovery (`None`); never advances the
    /// queue, unlike a decode error.
    DeviceError(Option<String>),
    LyricsLoaded {
        song_id: u64,
        lyrics: Vec<LyricLine>,
        translated_lyrics: Vec<LyricLine>,
    },
    HeartbeatSong {
        request: u64,
        song: SongInfo,
    },
    HeartbeatFallback {
        request: u64,
    },
    SetPlaylistId(u64),
    LikeSong(u64, bool),
    DislikeSong(u64),
    Cached(u64),
    /// The cloud "我喜欢的音乐" list has been fetched/updated; the main thread refreshes `PlaybackState.liked` from it.
    LikedUpdated,
    /// Append lazily-paged songs to the queue identified by `key` (background
    /// full-list load after Enter on a playlist page). Songs are shared `Arc`s
    /// so appending never deep-clones the page.
    QueueAppend {
        key: String,
        songs: Vec<Arc<SongInfo>>,
    },
    /// The full song list for `playlist_id` has been queued; subsequent Enter
    /// presses on the same playlist can jump within the queue without reloading.
    QueueLoadDone {
        playlist_id: u64,
    },
}
