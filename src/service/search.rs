//! Shared search execution used by the TUI search bar and the IPC `search`
//! request, plus the underlying NCM song lookup on `ApiService`.
//!
//! The TUI search is async-fire-and-forget (spawns a task, pushes
//! [`crate::event::NavigationEvent`]s, updates navigation state) while the IPC
//! server must answer `pigma msg search` synchronously, so the *orchestration*
//! lives apart (`app/search.rs` for the TUI, `ipc/server.rs` for IPC) — but the
//! actual search execution, result conversion and registration are shared
//! here: both paths call [`search_ncm`] / [`search_sonar`].

use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    sync::{Arc, Mutex},
};

use ncm_api::SongInfo;
use sonar::{SearchQuery, SonarFinder, Song};

use super::{ApiService, api::arc_songs};
use crate::{
    lock::lock_recover,
    state::{ContentState, SearchProvider},
};

/// Upper bound for the recently-searched result registries. They only need to
/// keep enough results to resolve `pigma msg play <id>` for recent searches;
/// without a cap they grow for the whole session.
pub const SEARCH_REGISTRY_CAPACITY: usize = 1024;

/// Small insertion-ordered, bounded map used by the search registries.
///
/// A plain `HashMap` cannot evict in insertion order and an LRU dependency
/// would be overkill for two internal maps, so this keeps a key queue alongside
/// the map. Re-inserting an existing key updates its value without duplicating
/// the queue entry.
pub struct SearchRegistry<K, V> {
    map: HashMap<K, V>,
    order: VecDeque<K>,
    capacity: usize,
}

impl<K: Eq + Hash + Clone, V> SearchRegistry<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    pub fn insert(&mut self, key: K, value: V) {
        if self.map.insert(key.clone(), value).is_none() {
            self.order.push_back(key);
            while self.map.len() > self.capacity {
                let Some(oldest) = self.order.pop_front() else {
                    break;
                };
                self.map.remove(&oldest);
            }
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

impl<K: Eq + Hash + Clone, V> Default for SearchRegistry<K, V> {
    fn default() -> Self {
        Self::new(SEARCH_REGISTRY_CAPACITY)
    }
}

/// Registry of recently searched NCM/third-party songs keyed by song id,
/// shared with `App` so `pigma msg play <id>` can enqueue and play a result
/// that is not part of the active playback queue.
pub type SearchResults = Arc<Mutex<SearchRegistry<u64, Arc<SongInfo>>>>;

/// Original sonar songs for search results, keyed by the synthetic id used by
/// playback to resolve the provider URL.
pub type SonarSongs = Arc<Mutex<SearchRegistry<u64, Arc<Song>>>>;

/// A search hit: the shared [`SongInfo`] (what gets queued/played) and the
/// provider tag. The `Arc` lets the NCM result list and the search-hit vector
/// share the same song without deep-cloning its strings.
pub struct SearchHit {
    pub info: Arc<SongInfo>,
    pub source: SearchProvider,
}

/// Searches NCM and every enabled sonar source on behalf of the IPC server,
/// registering results so their synthetic ids stay resolvable in this instance.
pub struct SearchEngine {
    service: ApiService,
    finder: Arc<SonarFinder>,
    sonar_songs: SonarSongs,
    search_results: SearchResults,
    limit: usize,
    providers: Vec<SearchProvider>,
}

impl ApiService {
    /// Search songs by keyword on the NCM client and share the result as
    /// `Arc`s so the content state and playback queue own the same entries.
    pub async fn search_songs(&self, keyword: &str, limit: u16) -> ContentState {
        match self.client.search_song(keyword, 0, limit).await {
            Ok(result) => ContentState::Songs(arc_songs(result.songs)),
            Err(e) => ContentState::Error(e.to_string()),
        }
    }
}

impl SearchEngine {
    pub fn new(
        service: ApiService,
        finder: Arc<SonarFinder>,
        sonar_songs: SonarSongs,
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
    pub async fn search(&self, keyword: &str) -> Vec<SearchHit> {
        let mut hits = Vec::with_capacity(self.limit.saturating_mul(self.providers.len()));

        if self.providers.contains(&SearchProvider::Ncm) {
            match search_ncm(&self.service, &self.search_results, keyword, self.limit).await {
                ContentState::Songs(songs) => {
                    hits.extend(songs.iter().map(|song| SearchHit {
                        info: Arc::clone(song),
                        source: SearchProvider::Ncm,
                    }));
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
                Ok(found) => hits.extend(found),
                Err(e) => {
                    log::warn!("sonar search failed: {e}");
                }
            }
        }

        hits
    }
}

/// Search NetEase Cloud Music for `keyword` and register the hits by id so
/// `pigma msg play <id>` can enqueue them later. Returns the API `ContentState`
/// unchanged (the TUI surfaces the error string verbatim).
pub(crate) async fn search_ncm(
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
pub(crate) async fn search_sonar(
    finder: &SonarFinder,
    sonar_songs: &SonarSongs,
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
                let info = Arc::new(to_song_info(&song));
                let source = SearchProvider::from_sonar(song.source);
                lock_recover(sonar_songs).insert(info.id, Arc::new(song));
                lock_recover(search_results).insert(info.id, Arc::clone(&info));
                SearchHit { info, source }
            })
            .collect()),
        Err(e) => Err(format!("搜索失败: {e}")),
    }
}

/// Map a sonar search result onto the app's `SongInfo`. The synthetic id is
/// generated by sonar itself (see `sonar::make_song_id`) so playback can
/// route through the sonar fallback (`crate::playback::source`).
fn to_song_info(song: &Song) -> SongInfo {
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
fn register_search_results(search_results: &SearchResults, songs: &[Arc<SongInfo>]) {
    let mut map = lock_recover(search_results);
    for song in songs {
        map.insert(song.id, Arc::clone(song));
    }
}

#[cfg(test)]
mod tests {
    use super::SearchRegistry;

    #[test]
    fn registry_evicts_the_oldest_insertion_and_updates_existing_keys() {
        let mut registry = SearchRegistry::new(2);
        registry.insert(1, "a");
        registry.insert(2, "b");
        registry.insert(3, "c");

        assert_eq!(registry.get(&1), None);
        assert_eq!(registry.get(&2), Some(&"b"));
        assert_eq!(registry.get(&3), Some(&"c"));
        assert_eq!(registry.len(), 2);

        // Updating an existing key must not add a duplicate insertion entry.
        registry.insert(2, "B");
        assert_eq!(registry.get(&2), Some(&"B"));
        assert_eq!(registry.len(), 2);
    }
}
