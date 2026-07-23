use super::{App, send_event};
use crate::event::NavigationEvent;
use crate::state::ContentState;

impl App {
    pub(super) fn handle_search_song(&mut self, keyword: String) {
        self.state.navigation.set_content(ContentState::Loading);
        self.state.navigation.nav.subtitle = Some(format!("搜索: {keyword}"));
        self.state.navigation.content_selected = 0;
        let api = self.api.clone();
        let sender = self.state.events.sender();
        let limit = self.config.search_limit;
        tokio::spawn(async move {
            let result = api.search_song(&keyword, 0, limit).await;
            let state = match result {
                Ok(r) => ContentState::Songs(r.songs),
                Err(e) => ContentState::Error(e.to_string()),
            };
            send_event(&sender, NavigationEvent::ContentLoaded(state).into());
        });
    }

    pub(super) fn handle_search_activate(&mut self) {
        let nav = &mut self.state.navigation;
        nav.search.active = true;
        nav.search.input = crate::text_input::TextInput::new();
        nav.search.filter_queue_only = false;
        nav.search.unfiltered_songs = None;
        nav.search.unfiltered_songs_lower = None;

        nav.push_breadcrumb();

        nav.nav.subtitle = None;
        nav.content_selected = 0;

        nav.nav.restore_focus_by_api("search");

        nav.set_content(ContentState::Empty);

        let api = nav
            .nav
            .section_states
            .get(nav.nav.focus_section)
            .and_then(|st| st.selected())
            .and_then(|i| nav.nav.sections.get(nav.nav.focus_section)?.items.get(i))
            .and_then(|item| item.api.as_ref());
        if let Some(api) = api {
            let sender = self.state.events.sender();
            send_event(&sender, NavigationEvent::NavSelect(api.clone()).into());
        }
    }

    pub(super) fn handle_search_deactivate(&mut self) {
        let nav = &mut self.state.navigation;
        if nav.search.filter_queue_only {
            nav.search.filter_queue_only = false;
            if let Some(songs) = nav.search.unfiltered_songs.take() {
                self.playback.set_queue_songs(songs);
            }
            nav.search.unfiltered_songs_lower = None;
        } else {
            nav.pop_breadcrumb();
        }
        nav.search.active = false;
        nav.search.input = crate::text_input::TextInput::new();
        nav.nav.subtitle = None;
    }

    pub(super) fn handle_content_restore(&mut self) {
        let nav = &mut self.state.navigation;
        nav.pop_breadcrumb();
    }
}
