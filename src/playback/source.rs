//! Audio source resolution: cached files, local files, NCM streaming and the
//! sonar fallback. Streaming setup lives in `stream`, provider resolution in
//! `resolve`.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use ncm_api::SongQuality;
use sonar::{SonarFinder, Song};
use tokio::sync::mpsc;

use crate::{cache::CacheManager, event::Event, service::ApiService};

mod resolve;
mod stream;

#[derive(Clone)]
pub(super) struct AudioSource {
    pub(super) service: ApiService,
    pub cache: Arc<CacheManager>,
    pub(super) quality: SongQuality,
    /// Save while playing: when true, stream and write the file into the download cache; when
    /// false, stream to a temporary file only.
    pub(super) save_on_play: bool,
    pub(super) finder: Arc<SonarFinder>,
    pub(super) sonar_enabled: bool,
    /// HTTP client used for streaming play URLs (proxy + headers applied).
    pub(super) stream_client: reqwest::Client,
    pub(super) event_tx: mpsc::UnboundedSender<Event>,
    /// Original sonar songs for search results, keyed by the synthetic
    /// `SongInfo` id so playback can resolve the source via the same provider.
    pub(super) sonar_songs: Arc<Mutex<HashMap<u64, Arc<Song>>>>,
}

impl AudioSource {
    // AudioSource wires several shared services; a params struct is deferred until a second constructor appears.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        service: ApiService,
        cache: Arc<CacheManager>,
        quality: SongQuality,
        save_on_play: bool,
        stream_client: reqwest::Client,
        finder: Arc<SonarFinder>,
        sonar_enabled: bool,
        sonar_songs: Arc<Mutex<HashMap<u64, Arc<Song>>>>,
        event_tx: mpsc::UnboundedSender<Event>,
    ) -> Self {
        Self {
            service,
            cache,
            quality,
            save_on_play,
            finder,
            sonar_enabled,
            stream_client,
            event_tx,
            sonar_songs,
        }
    }

    /// Toggle save-on-play at runtime: when true, stream and write the file into the download
    /// cache; when false, stream to a temporary file only.
    pub(super) fn set_save_on_play(&mut self, enabled: bool) {
        self.save_on_play = enabled;
    }
}
