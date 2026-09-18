//! Shared search core used by the TUI search bar and the IPC `search` request.
//!
//! The TUI search is async-fire-and-forget (spawns a task, pushes
//! [`crate::event::NavigationEvent`]s, updates navigation state) while the
//! IPC server must answer `pigma msg search` synchronously, so the *orchestration*
//! lives apart (see `super::search` for the TUI side) — but the actual search
//! execution, result conversion and registration are shared here: both paths
//! call [`search_ncm`] / [`search_sonar`].

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use ncm_api::SongInfo;
use sonar::{SearchQuery, SonarFinder, Song};

use crate::{
    ipc::SearchEntry,
    service::ApiService,
    state::{ContentState, SearchProvider},
};

/// Registry of recently searched songs keyed by song id, shared with `App` so
/// `pigma msg play <id>` can enqueue and play a result that is not part of the
/// active playback queue.
pub type SearchResults = Arc<Mutex<HashMap<u64, Arc<SongInfo>>>>;

/// Grouped search subsystem owned by `App`: the shared sonar finder and
/// synthetic-id registry, the recently-searched result registry backing
/// `pigma msg play <id>`, and the cross-provider engine serving
/// `pigma msg search <keyword>`.
pub struct SearchHost {
    pub finder: Arc<SonarFinder>,
    pub sonar_songs: Arc<Mutex<HashMap<u64, Arc<Song>>>>,
    pub results: SearchResults,
    pub engine: Arc<SearchEngine>,
}

/// A sonar search hit: the converted [`SongInfo`] (what gets queued/played)
/// and the provider tag. The original song lives in the `sonar_songs` registry
/// keyed by its synthetic id, which is how playback later resolves a play URL.
pub struct SearchHit {
    pub info: SongInfo,
    pub source: String,
}

/// Searches NCM and every enabled sonar source on behalf of the IPC server,
/// registering results so their synthetic ids stay resolvable in this instance.
pub struct SearchEngine {
    service: ApiService,
    finder: Arc<SonarFinder>,
    sonar_songs: Arc<Mutex<HashMap<u64, Arc<Song>>>>,
    search_results: SearchResults,
    limit: usize,
    providers: Vec<SearchProvider>,
}

impl SearchEngine {
    pub fn new(
        service: ApiService,
        finder: Arc<SonarFinder>,
        sonar_songs: Arc<Mutex<HashMap<u64, Arc<Song>>>>,
        search_results: SearchResults,
        limit: usize,
        providers: Vec<SearchProvider>,
    ) -> Self {
        Self {
            service,
            finder,
            sonar_songs,
            search_results,
            limit,
            providers,
        }
    }

    /// Run a search across NCM (when enabled) and the enabled sonar sources,
    /// sharing the same helpers as the TUI search bar.
    pub async fn search(&self, keyword: &str) -> Vec<SearchEntry> {
        let mut entries = Vec::new();

        if self.providers.contains(&SearchProvider::Ncm) {
            match search_ncm(&self.service, &self.search_results, keyword, self.limit).await {
                ContentState::Songs(songs) => {
                    for song in &songs {
                        entries.push(SearchEntry::from_song(song, "netease"));
                    }
                }
                ContentState::Error(e) => {
                    log::warn!("NCM search failed: {e}");
                }
                _ => {}
            }
        }

        if self.providers.iter().any(|p| *p != SearchProvider::Ncm) {
            match search_sonar(
                &self.finder,
                &self.sonar_songs,
                &self.search_results,
                keyword,
                self.limit,
            )
            .await
            {
                Ok(hits) => {
                    for hit in hits {
                        entries.push(SearchEntry::from_song(&hit.info, &hit.source));
                    }
                }
                Err(e) => {
                    log::warn!("sonar search failed: {e}");
                }
            }
        }

        entries
    }
}

/// Search NetEase Cloud Music for `keyword` and register the hits by id so
/// `pigma msg play <id>` can enqueue them later. Returns the API `ContentState`
/// unchanged (the TUI surfaces the error string verbatim).
pub async fn search_ncm(
    service: &ApiService,
    search_results: &SearchResults,
    keyword: &str,
    limit: usize,
) -> ContentState {
    match service.search_songs(keyword, limit as u16).await {
        ContentState::Songs(songs) => {
            register_search_results(search_results, &songs);
            ContentState::Songs(songs)
        }
        other => other,
    }
}

/// Search the sonar providers in `finder` (the TUI passes a single-provider
/// finder to restrict to the selected source; the IPC engine passes the shared
/// all-provider finder). Registers every hit in the shared registries so a
/// later `pigma msg play <id>` resolves. Returns `Err` with the raw error text.
pub async fn search_sonar(
    finder: &SonarFinder,
    sonar_songs: &Arc<Mutex<HashMap<u64, Arc<Song>>>>,
    search_results: &SearchResults,
    keyword: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    match finder.search(&SearchQuery::new(keyword)).await {
        Ok(found) => Ok(found
            .songs
            .into_iter()
            .take(limit)
            .map(|song| {
                let info = to_song_info(&song);
                let source = SearchProvider::from_sonar(song.source)
                    .display_name()
                    .to_string();
                if let Ok(mut map) = sonar_songs.lock() {
                    map.insert(info.id, Arc::new(song.clone()));
                }
                if let Ok(mut map) = search_results.lock() {
                    map.insert(info.id, Arc::new(info.clone()));
                }
                SearchHit { info, source }
            })
            .collect()),
        Err(e) => Err(format!("搜索失败: {e}")),
    }
}

/// Map a sonar search result onto the app's `SongInfo`. The synthetic id is
/// generated by sonar itself (see `sonar::make_song_id`) so playback can
/// route through the sonar fallback (`crate::playback::source`).
pub fn to_song_info(song: &Song) -> SongInfo {
    SongInfo {
        id: song.id,
        name: song.name.clone(),
        singer: song.singer.clone(),
        artist_id: 0,
        album: song.album.clone(),
        album_id: 0,
        pic_url: song.pic_url.clone(),
        duration: song.duration,
        copyright: ncm_api::SongCopyright::Free,
    }
}

/// Register a batch of search hits by id so `pigma msg play <id>` can later
/// enqueue and play them even though they are not part of the active queue.
pub fn register_search_results(search_results: &SearchResults, songs: &[Arc<SongInfo>]) {
    if let Ok(mut map) = search_results.lock() {
        for song in songs {
            map.insert(song.id, Arc::clone(song));
        }
    }
}
