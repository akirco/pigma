mod controller;
mod mode;
mod player;
mod queue;
mod source;
mod storage;
pub mod types;

use std::fs;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ncm_api::{NcmClient, SongInfo};
use rodio::Source;
use tokio::sync::mpsc;

use crate::types::{AppEvent, Event};

use self::controller::PlayerController;
use self::mode::PlayStrategy;
use self::queue::PlaylistQueue;
use self::source::AudioSource;
use self::storage::PlaylistStorage;
use self::types::{LyricLine, PlayMode, PlaybackState};

pub use self::types::{
    LyricLine as PlaybackLyricLine, PlayMode as EnginePlayMode, PlaybackState as EngineState,
    parse_lyric_lines,
};

pub struct PlaybackEngine {
    pub state: PlaybackState,
    pub queue: PlaylistQueue,
    strategy: Box<dyn PlayStrategy>,
    storage: PlaylistStorage,
    pub source: AudioSource,
    controller: PlayerController,
    event_tx: mpsc::UnboundedSender<Event>,
    api: Arc<NcmClient>,
    playlist_id: Option<u64>,
}

impl PlaybackEngine {
    pub fn new(event_tx: mpsc::UnboundedSender<Event>, api: Arc<NcmClient>) -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pigma")
            .join("songs");

        let storage = PlaylistStorage::new();
        let mut this = Self {
            state: PlaybackState::default(),
            queue: PlaylistQueue::new(),
            strategy: Box::new(mode::Sequential),
            storage,
            source: AudioSource::new(api.clone(), cache_dir),
            controller: PlayerController::new(event_tx.clone()),
            event_tx: event_tx.clone(),
            api,
            playlist_id: None,
        };
        this.restore_session();
        this
    }

    pub fn current_song(&self) -> Option<SongInfo> {
        self.queue.current_song().cloned()
    }

    pub fn song_at(&self, index: usize) -> Option<&SongInfo> {
        self.queue.songs.get(index)
    }

    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    pub fn queue_songs(&self) -> &[SongInfo] {
        &self.queue.songs
    }

    pub fn queue_history(&self) -> &[SongInfo] {
        &self.queue.history
    }

    pub fn play_songs(&mut self, songs: Vec<SongInfo>, index: usize) {
        if songs.is_empty() || index >= songs.len() {
            return;
        }
        self.controller.stop();
        self.queue = PlaylistQueue::from_songs(songs, index);
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
        self.start_current_song(None);
    }

    pub fn append_and_play(&mut self, songs: &[SongInfo], index: usize) {
        if songs.is_empty() || index >= songs.len() {
            return;
        }
        self.controller.stop();
        let offset = self.queue.append(songs);
        self.queue.current_index = Some(offset + index);
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
        self.start_current_song(None);
    }

    pub fn play_index(&mut self, index: usize) {
        if index >= self.queue.len() {
            return;
        }
        self.controller.stop();
        if let Some(old_idx) = self.queue.current_index
            && let Some(song) = self.queue.songs.get(old_idx)
        {
            self.queue.history.push(song.clone());
        }
        self.queue.current_index = Some(index);
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
        self.start_current_song(None);
    }

    pub fn next(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        if matches!(self.state.mode, PlayMode::Heartbeat { .. }) {
            // FIXME(Heartbeat): `next_heartbeat` 仅 spawn 异步请求并立即返回，不会停止当前歌曲，
            // 也不会推进 current_index。当前歌曲播完后 on_playback_finished → next() 会再次
            // next_heartbeat()，导致多个 in-flight 请求交错、可能拿到过期/重复的下一首。
            self.next_heartbeat();
            return;
        }

        let next_idx = self
            .strategy
            .next(self.queue.current_index, &self.queue.songs);
        match next_idx {
            Some(idx) => {
                if Some(idx) != self.queue.current_index {
                    self.queue.push_to_history();
                }
                self.queue.current_index = Some(idx);
                self.start_current_song(None);
            }
            None => {
                self.stop();
            }
        }
    }

    pub fn prev(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        if let Some(prev_song) = self.queue.pop_history()
            && let Some(pos) = self.queue.find_song_index(prev_song.id)
        {
            self.queue.current_index = Some(pos);
            self.start_current_song(None);
            return;
        }

        let prev_idx = self
            .strategy
            .prev(self.queue.current_index, &self.queue.songs);
        if let Some(idx) = prev_idx {
            self.queue.current_index = Some(idx);
            self.start_current_song(None);
        }
    }

    pub fn toggle_pause(&mut self) {
        if !self.state.playing && self.queue.current_index.is_some() {
            let seek_time = if self.state.progress > 0.0 {
                self.queue.current_song().and_then(|s| {
                    let secs = self.state.progress * (s.duration as f64 / 1000.0);
                    (secs > 0.0).then(|| Duration::from_secs_f64(secs))
                })
            } else {
                None
            };
            self.start_current_song(seek_time);
            return;
        }
        if self.state.paused {
            self.controller.resume();
        } else {
            self.controller.pause();
        }
        self.state.paused = !self.state.paused;
    }

    pub fn stop(&mut self) {
        self.controller.stop();
        self.queue.current_index = None;
        self.state.playing = false;
        self.state.paused = false;
        self.state.current_song = None;
        self.state.progress = 0.0;
    }

    pub fn seek_relative(&mut self, delta_secs: f64) {
        let duration = match self.queue.current_song() {
            Some(s) => s.duration,
            None => return,
        };
        let total_secs = duration as f64 / 1000.0;
        if total_secs <= 0.0 {
            return;
        }

        if !self.state.playing {
            let cur_secs = self.state.progress * total_secs;
            self.state.progress =
                ((cur_secs + delta_secs).clamp(0.0, total_secs) / total_secs).clamp(0.0, 1.0);
            return;
        }

        let cur_secs = self.state.progress * total_secs;
        let new_secs = (cur_secs + delta_secs).clamp(0.0, total_secs);
        self.state.progress = new_secs / total_secs;
        self.state.seeking = true;
        self.controller.seek_to(Duration::from_secs_f64(new_secs));
    }

    pub fn set_volume(&mut self, volume: f64) {
        self.state.volume = volume;
        self.controller.set_volume(volume as f32);
    }

    pub fn volume(&self) -> f64 {
        self.state.volume
    }

    pub fn set_playlist_id(&mut self, id: u64) {
        self.playlist_id = Some(id);
    }

    pub fn cycle_mode(&mut self) {
        let next = match self.state.mode {
            PlayMode::Sequential => PlayMode::RepeatOne,
            PlayMode::RepeatOne => PlayMode::RepeatAll,
            PlayMode::RepeatAll => PlayMode::Shuffle,
            PlayMode::Shuffle => match self.playlist_id {
                Some(pid) if pid > 0 => PlayMode::Heartbeat { playlist_id: pid },
                _ => PlayMode::Sequential,
            },
            PlayMode::Heartbeat { .. } => PlayMode::Sequential,
        };
        self.set_mode(next);
    }

    pub fn set_mode(&mut self, mode: PlayMode) {
        self.state.mode = mode.clone();
        self.strategy =
            mode::create_strategy(&self.state.mode, self.queue.len(), self.queue.current_index);
    }

    pub fn handle_finished(&mut self) {
        if !self.state.playing {
            return;
        }
        self.next();
    }

    pub fn on_playback_started(&mut self) {
        self.state.error = None;
        self.state.paused = false;
        self.state.playing = true;
        self.state.lyrics = None;
        self.state.translated_lyrics = None;
    }

    pub fn on_playback_progress(&mut self, position: Duration, total: Option<Duration>) {
        self.state.seeking = false;
        let total_secs = match total {
            Some(t) => t.as_secs_f64(),
            None => self
                .queue
                .current_song()
                .map(|s| s.duration as f64 / 1000.0)
                .unwrap_or(0.0),
        };
        if total_secs > 0.0 {
            self.state.progress = (position.as_secs_f64() / total_secs).clamp(0.0, 1.0);
        }
    }

    pub fn on_playback_finished(&mut self) {
        self.state.progress = 0.0;
        self.handle_finished();
        if !self.state.playing {
            self.state.current_song = None;
            self.state.error = None;
            self.state.paused = false;
        }
    }

    pub fn on_playback_error(&mut self, err: String) {
        log::error!("Playback error: {}", err);
        self.state.error = Some(err);
    }

    pub fn on_lyrics_loaded(
        &mut self,
        song_id: u64,
        lyrics: Vec<LyricLine>,
        translated_lyrics: Vec<LyricLine>,
    ) {
        if let Some(song) = &self.state.current_song
            && song.id == song_id
        {
            self.state.lyrics = Some(lyrics);
            self.state.translated_lyrics = Some(translated_lyrics);
        }
    }

    pub fn save_session(&self) {
        self.storage.save_auto(
            &self.queue.songs,
            &self.queue.history,
            self.queue.current_index,
            &self.state.mode,
            self.state.volume,
            self.state.progress,
        );
    }

    pub fn save_playlist(&self, name: &str) -> bool {
        self.storage.save_playlist(name, &self.queue.songs)
    }

    pub fn load_playlist(&mut self, name: &str) -> bool {
        match self.storage.load_playlist(name) {
            Some(songs) if !songs.is_empty() => {
                self.play_songs(songs, 0);
                true
            }
            _ => false,
        }
    }

    pub fn delete_playlist(&self, name: &str) -> bool {
        self.storage.delete_playlist(name)
    }

    pub fn list_playlists(&self) -> Vec<String> {
        self.storage.list_playlists()
    }

    fn restore_session(&mut self) {
        if let Some(saved) = self.storage.load_auto()
            && !saved.queue.is_empty()
        {
            self.queue = PlaylistQueue {
                songs: saved.queue,
                history: saved.history,
                current_index: saved.current_index,
            };
            self.state.mode = saved.mode.clone();
            self.state.volume = saved.volume;
            self.strategy =
                mode::create_strategy(&saved.mode, self.queue.len(), self.queue.current_index);
            self.controller.set_volume(saved.volume as f32);

            if saved.current_index.is_some() {
                self.state.current_song = self.queue.current_song().cloned().map(Arc::new);
                self.state.progress = saved.progress;
            }
        }
    }

    fn start_current_song(&mut self, seek_time: Option<Duration>) {
        let song = match self.queue.current_song() {
            Some(s) => Arc::new(s.clone()),
            None => return,
        };

        self.state.playing = true;
        self.state.paused = false;
        self.state.seeking = true;
        self.state.current_song = Some(song.clone());
        self.state.error = None;
        self.state.cached = self.source.cache.is_cached(song.id, "mp3");

        if let Some(t) = seek_time {
            let total_secs = song.duration as f64 / 1000.0;
            if total_secs > 0.0 {
                self.state.progress = (t.as_secs_f64() / total_secs).clamp(0.0, 1.0);
            }
        }

        let source = self.source.clone();
        let event_tx = self.event_tx.clone();
        let controller = self.controller.clone();

        if event_tx
            .send(Event::App(AppEvent::PlaybackStarted))
            .is_err()
        {
            log::error!("Failed to send PlaybackStarted: receiver dropped");
        }

        tokio::spawn(async move {
            let input = match source.resolve(&song).await {
                Ok(input) => input,
                Err(e) => {
                    if event_tx
                        .send(Event::App(AppEvent::PlaybackError(e)))
                        .is_err()
                    {
                        log::error!("Failed to send PlaybackError: receiver dropped");
                    }
                    return;
                }
            };
            controller.request(input, seek_time);
        });
    }

    // FIXME(Heartbeat): 已知问题 ——
    // 1. 当前歌曲播完前不会 stop，会重复触发 next_heartbeat 产生并发请求；
    // 2. playmode_intelligence_list 返回列表只取第一首(next())，其余丢弃，
    //    没有真正维护"心动队列"，每次切歌都再发一次网络请求；
    // 3. HeartbeatSong 异步回传与 on_playback_finished 之间存在竞态，
    //    快速切歌时可能播放到过时推荐。
    fn next_heartbeat(&mut self) {
        let song = match self.queue.current_song() {
            Some(s) => s.clone(),
            None => return,
        };
        let playlist_id = match self.state.mode {
            PlayMode::Heartbeat { playlist_id } => playlist_id,
            _ => return,
        };

        log::info!("Heartbeat: song_id={} playlist_id={}", song.id, playlist_id);

        let api = self.api.clone();
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            match api.playmode_intelligence_list(song.id, playlist_id).await {
                Ok(songs) => {
                    // FIXME(Heartbeat): 只取推荐列表第一首，其余丢弃，没有维护"心动队列"。
                    // 建议缓存整段推荐队列，本地推进，仅在队列耗尽时再请求。
                    if let Some(next_song) = songs.into_iter().next() {
                        if event_tx
                            .send(Event::App(AppEvent::HeartbeatSong(next_song)))
                            .is_err()
                        {
                            log::error!("Failed to send HeartbeatSong: receiver dropped");
                        }
                    } else if event_tx
                        .send(Event::App(AppEvent::HeartbeatFallback))
                        .is_err()
                    {
                        log::error!("Failed to send HeartbeatFallback: receiver dropped");
                    }
                }
                Err(e) => {
                    log::warn!("Heartbeat failed: {e}, falling back to queue");
                    if event_tx
                        .send(Event::App(AppEvent::HeartbeatFallback))
                        .is_err()
                    {
                        log::error!("Failed to send HeartbeatFallback: receiver dropped");
                    }
                }
            }
        });

        // NOTE(Heartbeat): 故意保持 state.playing=true，避免 on_playback_finished 清掉
        // current_song。但这样会与上面 FIXME 的竞态叠加：歌曲已结束、progress 已归零、
        // 下一首尚未送达期间，用户操作或再次触发 next 会产生未定义行为。
    }

    // FIXME(Heartbeat): 从 AppEvent(HeartbeatSong) 异步回调进入，先 stop 再 push 新歌。
    // 多个并发的 next_heartbeat 请求可能依次送达，导致一首歌被"下一首"覆盖多次，
    // 或播放顺序与用户预期不符。
    pub fn play_heartbeat_song(&mut self, song: SongInfo) {
        self.controller.stop();
        self.queue.push_to_history();
        self.queue.songs.push(song);
        self.queue.current_index = Some(self.queue.len() - 1);
        self.start_current_song(None);
    }

    pub fn on_heartbeat_fallback(&mut self) {
        log::info!("Heartbeat fallback: switching to sequential");
        self.set_mode(PlayMode::Sequential);
        self.next();
    }
}

pub fn scan_local_music(dir: &std::path::Path) -> Vec<SongInfo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return vec![];
    };
    let extensions = ["mp3", "flac", "wav", "ogg", "aac", "m4a", "wma"];
    let mut songs = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        if !extensions.contains(&ext.as_str()) {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let duration = fs::File::open(&path)
            .ok()
            .and_then(|f| rodio::Decoder::new(BufReader::new(f)).ok())
            .and_then(|d| d.total_duration())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        songs.push(SongInfo {
            id: 0,
            name,
            singer: "本地".into(),
            artist_id: 0,
            album: path.to_string_lossy().to_string(),
            album_id: 0,
            pic_url: String::new(),
            duration,
            copyright: ncm_api::SongCopyright::Free,
        });
    }
    songs.sort_by(|a, b| a.name.cmp(&b.name));
    songs
}
