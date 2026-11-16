//! TUI search-bar orchestration: turns keyboard input into navigation state and
//! spawned searches, surfacing results via [`crate::event::NavigationEvent`]s.
//! The execution shared with the IPC `search` server lives in
//! [`crate::service::search`].

use std::sync::Arc;

use sonar::SonarFinder;

use super::{App, builder::ProxyKind, event::send_event};
use crate::{
    config::Config,
    event::NavigationEvent,
    service::{ApiEndpoint, SearchEngine, SearchResults, SonarSongs, search_ncm, search_sonar},
    state::{ContentState, SearchProvider},
    text_input::TextInput,
};

/// Build a single-provider search config that still honours the configured
/// proxies and timeout. The shared finder used for playback already has these,
/// but the TUI creates a one-provider finder for the search bar.
fn sonar_search_config(config: &Config, source: sonar::SonarSource) -> sonar::SearchConfig {
    sonar::SearchConfig::new()
        .with_providers(vec![source])
        .with_timeout(config.source_fallback.timeout_ms)
        .with_search_proxy(App::proxy_for(config, ProxyKind::NonYoutube))
        .with_youtube_proxy(App::proxy_for(config, ProxyKind::Youtube))
}

/// Search subsystem owned by `App`: the shared sonar finder and synthetic-id
/// registry, the recently-searched result registry backing `pigma msg play
/// <id>`, and the cross-provider engine serving `pigma msg search <keyword>`.
pub(super) struct SearchHost {
    pub(super) finder: Arc<SonarFinder>,
    pub(super) sonar_songs: SonarSongs,
    pub(super) results: SearchResults,
    pub(super) engine: Arc<SearchEngine>,
}

impl App {
    pub(super) fn handle_search_song(&mut self, keyword: String) {
        let generation = self.bump_content_generation();
        match self.state.navigation.search.provider {
            SearchProvider::Ncm => self.submit_ncm_search(keyword, generation),
            provider => self.submit_sonar_search(keyword, provider, generation),
        }
    }

    /// TUI-only orchestration for an NCM search: mark the loading state, spawn
    /// the search (delegating to [`search_ncm`]) and hand the
    /// resulting `ContentState` to the navigation via an event.
    fn submit_ncm_search(&mut self, keyword: String, generation: u64) {
        self.state.navigation.set_content(ContentState::Loading);
        self.state.navigation.content_is_search = true;
        self.state.navigation.nav.subtitle = Some(format!("搜索: {keyword}"));
        self.state.navigation.content_selected = 0;
        let service = self.service.clone();
        let sender = self.events.sender();
        let limit = self.config.search_limit as usize;
        let search_results = self.search.results.clone();
        tokio::spawn(async move {
            let state = search_ncm(&service, &search_results, &keyword, limit).await;
            send_event(
                &sender,
                NavigationEvent::ContentLoaded {
                    content: state,
                    generation,
                }
                .into(),
            );
        });
    }

    /// TUI-only orchestration for a single-source sonar search: build a finder
    /// restricted to the selected provider, then delegate to
    /// [`search_sonar`] and surface the result via an event.
    fn submit_sonar_search(&mut self, keyword: String, provider: SearchProvider, generation: u64) {
        let Some(source) = provider.to_sonar() else {
            log::warn!("sonar search requested for non-sonar provider");
            return;
        };
        self.state.navigation.set_content(ContentState::Loading);
        self.state.navigation.content_is_search = true;
        self.state.navigation.nav.subtitle =
            Some(format!("({}): {keyword}", provider.display_name()));
        self.state.navigation.content_selected = 0;
        let sender = self.events.sender();
        let registry = self.search.sonar_songs.clone();
        let search_results = self.search.results.clone();
        let limit = self.config.search_limit as usize;
        let config = sonar_search_config(&self.config, source);
        tokio::spawn(async move {
            let finder = match sonar::SonarFinder::new(config) {
                Ok(f) => f,
                Err(e) => {
                    send_event(
                        &sender,
                        NavigationEvent::ContentLoaded {
                            content: ContentState::Error(format!("搜索初始化失败: {e}")),
                            generation,
                        }
                        .into(),
                    );
                    return;
                }
            };
            let state =
                match search_sonar(&finder, &registry, &search_results, &keyword, limit).await {
                    Ok(hits) => {
                        let songs: Vec<Arc<ncm_api::SongInfo>> =
                            hits.into_iter().map(|h| h.info).collect();
                        if songs.is_empty() {
                            ContentState::Error("没有找到结果".into())
                        } else {
                            ContentState::Songs(songs)
                        }
                    }
                    Err(e) => ContentState::Error(e),
                };
            send_event(
                &sender,
                NavigationEvent::ContentLoaded {
                    content: state,
                    generation,
                }
                .into(),
            );
        });
    }

    pub(super) fn handle_search_activate(&mut self) {
        // Invalidate any content load that is still in flight before entering
        // search; otherwise its response could land behind the empty page.
        self.bump_content_generation();
        let nav = &mut self.state.navigation;
        nav.search.active = true;
        nav.search.input = TextInput::new();
        nav.search.filter_queue_only = false;
        nav.search.unfiltered_songs = None;
        nav.search.provider = SearchProvider::Ncm;

        nav.push_breadcrumb();

        nav.nav.subtitle = None;
        nav.content_selected = 0;

        nav.nav.restore_focus_by_api(ApiEndpoint::Search.as_str());

        nav.set_content(ContentState::Empty);

        let api = nav.nav.selected_api();
        if let Some(api) = api {
            let sender = self.events.sender();
            send_event(&sender, NavigationEvent::NavSelect(api.to_string()).into());
        }
    }

    pub(super) fn handle_search_deactivate(&mut self) {
        // Drop search responses that are still in flight while leaving search.
        self.bump_content_generation();
        let nav = &mut self.state.navigation;
        if nav.search.filter_queue_only {
            nav.search.filter_queue_only = false;
            if let Some(songs) = nav.search.unfiltered_songs.take() {
                self.playback.set_queue_songs(songs);
            }
        } else {
            nav.pop_breadcrumb();
        }
        nav.search.active = false;
        nav.search.input = TextInput::new();
        nav.nav.subtitle = None;
    }

    /// Leave the in-queue filter (Enter while filtering): restore the unfiltered
    /// queue and put the playhead back on whichever song was selected while the
    /// list was filtered.
    pub(super) fn handle_queue_filter_exit(&mut self) {
        let selected = self.state.navigation.playlist_selected;
        let song_id = self.playback.song_at(selected).map(|s| s.id);
        let restored = self
            .state
            .navigation
            .search
            .unfiltered_songs
            .take()
            .unwrap_or_default();
        self.playback.set_queue_songs(restored);

        let nav = &mut self.state.navigation;
        nav.search.active = false;
        nav.search.filter_queue_only = false;
        nav.search.unfiltered_songs = None;
        nav.playlist_selected = 0;

        if let Some(id) = song_id
            && let Some(pos) = self.playback.queue_songs().iter().position(|s| s.id == id)
        {
            self.playback.play_index(pos);
        }
    }

    /// Recompute the filtered queue for the current keyword. An empty keyword
    /// restores every index of the unfiltered list.
    pub(super) fn handle_queue_filter_changed(&mut self) {
        let keyword = self.state.navigation.search.input.value.to_lowercase();
        let Some(full) = self.state.navigation.search.unfiltered_songs.as_ref() else {
            return;
        };
        let indices: Vec<usize> = if keyword.is_empty() {
            (0..full.len()).collect()
        } else {
            full.iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.name.to_lowercase().contains(&keyword)
                        || s.singer.to_lowercase().contains(&keyword)
                })
                .map(|(i, _)| i)
                .collect()
        };
        self.playback.set_queue_indices(full, &indices);
        self.state.navigation.playlist_selected = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProxyTarget;

    #[test]
    fn one_provider_search_keeps_proxies_and_configured_timeout() {
        let config = Config {
            proxy: "http://127.0.0.1:7890".into(),
            proxy_target: ProxyTarget::Both,
            source_fallback: crate::config::SonarConfig {
                timeout_ms: 1234,
                ..Default::default()
            },
            ..Config::default()
        };

        let search = sonar_search_config(&config, sonar::SonarSource::Youtube);

        assert_eq!(search.timeout_ms, 1234);
        assert_eq!(search.search_proxy, "http://127.0.0.1:7890");
        assert_eq!(search.youtube_proxy, "http://127.0.0.1:7890");
        assert_eq!(search.providers, vec![sonar::SonarSource::Youtube]);
    }
}
