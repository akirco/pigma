use std::sync::Arc;

use super::{
    App,
    event::send_event,
    search_core::{search_ncm, search_sonar},
};
use crate::{
    event::NavigationEvent,
    state::{ContentState, SearchProvider},
    text_input::TextInput,
};

impl App {
    pub(super) fn handle_search_song(&mut self, keyword: String) {
        match self.state.navigation.search.provider {
            SearchProvider::Ncm => self.submit_ncm_search(keyword),
            provider => self.submit_sonar_search(keyword, provider),
        }
    }

    /// TUI-only orchestration for an NCM search: mark the loading state, spawn
    /// the search (delegating to [`search_core::search_ncm`]) and hand the
    /// resulting `ContentState` to the navigation via an event.
    fn submit_ncm_search(&mut self, keyword: String) {
        self.state.navigation.set_content(ContentState::Loading);
        self.state.navigation.content_is_search = true;
        self.state.navigation.nav.subtitle = Some(format!("搜索: {keyword}"));
        self.state.navigation.content_selected = 0;
        let service = self.service.clone();
        let sender = self.state.events.sender();
        let limit = self.config.search_limit as usize;
        let search_results = self.search.results.clone();
        tokio::spawn(async move {
            let state = search_ncm(&service, &search_results, &keyword, limit).await;
            send_event(&sender, NavigationEvent::ContentLoaded(state).into());
        });
    }

    /// TUI-only orchestration for a single-source sonar search: build a finder
    /// restricted to the selected provider, then delegate to
    /// [`search_core::search_sonar`] and surface the result via an event.
    fn submit_sonar_search(&mut self, keyword: String, provider: SearchProvider) {
        self.state.navigation.set_content(ContentState::Loading);
        self.state.navigation.content_is_search = true;
        self.state.navigation.nav.subtitle =
            Some(format!("({}): {keyword}", provider.display_name()));
        self.state.navigation.content_selected = 0;
        let source = provider.to_sonar().expect("sonar provider");
        let sender = self.state.events.sender();
        let registry = self.search.sonar_songs.clone();
        let search_results = self.search.results.clone();
        let limit = self.config.search_limit as usize;
        tokio::spawn(async move {
            let config = sonar::SearchConfig::new()
                .with_providers(vec![source])
                .with_timeout(15000);
            let finder = match sonar::SonarFinder::new(config) {
                Ok(f) => f,
                Err(e) => {
                    send_event(
                        &sender,
                        NavigationEvent::ContentLoaded(ContentState::Error(format!(
                            "搜索初始化失败: {e}"
                        )))
                        .into(),
                    );
                    return;
                }
            };
            let state =
                match search_sonar(&finder, &registry, &search_results, &keyword, limit).await {
                    Ok(hits) => {
                        let songs: Vec<Arc<ncm_api::SongInfo>> =
                            hits.into_iter().map(|h| Arc::new(h.info)).collect();
                        if songs.is_empty() {
                            ContentState::Error("没有找到结果".into())
                        } else {
                            ContentState::Songs(songs)
                        }
                    }
                    Err(e) => ContentState::Error(e),
                };
            send_event(&sender, NavigationEvent::ContentLoaded(state).into());
        });
    }

    pub(super) fn handle_search_activate(&mut self) {
        let nav = &mut self.state.navigation;
        nav.search.active = true;
        nav.search.input = TextInput::new();
        nav.search.filter_queue_only = false;
        nav.search.unfiltered_songs = None;
        nav.search.provider = SearchProvider::Ncm;

        nav.push_breadcrumb();

        nav.nav.subtitle = None;
        nav.content_selected = 0;

        nav.nav.restore_focus_by_api("search");

        nav.set_content(ContentState::Empty);

        let api = nav.nav.selected_api();
        if let Some(api) = api {
            let sender = self.state.events.sender();
            send_event(&sender, NavigationEvent::NavSelect(api.to_string()).into());
        }
    }

    pub(super) fn handle_search_deactivate(&mut self) {
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

    pub(super) fn handle_content_restore(&mut self) {
        let nav = &mut self.state.navigation;
        nav.pop_breadcrumb();
    }
}
