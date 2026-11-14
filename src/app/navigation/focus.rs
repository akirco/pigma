//! Navigation focus movement and page transitions: endpoint selection,
//! previous/next item wrapping, local/main page jumps and manual refresh.

use super::super::{App, event::send_event};
use crate::{event::NavigationEvent, service::ApiEndpoint, state::Page};

impl App {
    /// Manually refresh the current navigation item: skip the cache, refetch, and write the result back to the cache.
    pub(crate) fn reload_current_nav(&mut self) {
        let Some(api) = self.state.navigation.nav.selected_api().map(str::to_string) else {
            self.toast("无可用内容刷新".into());
            return;
        };

        match ApiEndpoint::parse(&api) {
            // Local/download content comes from disk, so a refetch is just a
            // rescan; there is nothing to request from the API.
            Some(ApiEndpoint::LocalMusic) => {
                self.toast(format!("{} 刷新本地音乐", crate::icons::REFRESH))
            }
            Some(ApiEndpoint::Download) => {
                self.toast(format!("{} 刷新下载", crate::icons::REFRESH))
            }
            _ => {
                let _ = self.handle_nav_select(api, true);
                self.toast(format!("{} 刷新当前内容", crate::icons::REFRESH));
            }
        }
    }

    pub(crate) fn navigate_to_local(&mut self) {
        self.state.navigation.page = Page::Main;
        self.state.navigation.nav.focus_section = 1;
        if let Some(s) = self.state.navigation.nav.sections.get(1)
            && let Some(i) = s.items.iter().position(|item| item.name == "本地音乐")
        {
            self.state.navigation.nav.section_states[1].select(Some(i));
        }
        self.state.navigation.nav.subtitle = Some("本地音乐".into());
        let sender = self.events.sender();
        send_event(
            &sender,
            NavigationEvent::NavSelect(ApiEndpoint::LocalMusic.as_str().into()).into(),
        );
        self.state.navigation.content_selected = 0;
    }

    pub(crate) fn navigate_to_main(&mut self) {
        self.state.navigation.page = Page::Main;

        // Browsable without a logged-in session (public endpoints).
        const PUBLIC_APIS: &[ApiEndpoint] = &[
            ApiEndpoint::TopSongList,
            ApiEndpoint::Toplist,
            ApiEndpoint::TopSingers,
            ApiEndpoint::Search,
        ];
        let public_only = !self.service.client().is_logged_in();

        let api = self
            .state
            .navigation
            .nav
            .sections
            .iter()
            .find_map(|s| {
                s.items.iter().find_map(|i| {
                    let api = i.api.as_deref()?;
                    let endpoint = ApiEndpoint::parse(api)?;
                    if public_only && !PUBLIC_APIS.contains(&endpoint) {
                        None
                    } else {
                        Some(api.to_string())
                    }
                })
            })
            .or_else(|| {
                self.state
                    .navigation
                    .nav
                    .sections
                    .first()
                    .and_then(|s| s.items.first())
                    .and_then(|i| i.api.clone())
            });

        if let Some(api) = api {
            self.state.navigation.nav.restore_focus_by_api(&api);
            let sender = self.events.sender();
            send_event(&sender, NavigationEvent::NavSelect(api).into());
        }
    }

    /// The endpoint of the focused nav item, if it is a known API endpoint
    /// (items that only navigate, e.g. `login`, yield `None`).
    pub(crate) fn selected_endpoint(&self) -> Option<ApiEndpoint> {
        self.state
            .navigation
            .nav
            .selected_api()
            .and_then(ApiEndpoint::parse)
    }

    /// Move the navigation focus to the previous item, wrapping across
    /// sections.
    pub(crate) fn nav_select_prev(&mut self) {
        let nav = &mut self.state.navigation.nav;
        if nav.sections.is_empty() {
            return;
        }
        let focus = nav.focus_section;
        let len = nav.sections[focus].items.len();
        if len == 0 {
            return;
        }
        let current = nav.section_states[focus].selected().unwrap_or(0);
        if current > 0 {
            nav.section_states[focus].select(Some(current - 1));
        } else {
            let section_count = nav.sections.len();
            let prev_section = (focus + section_count - 1) % section_count;
            if nav.sections[prev_section].items.is_empty() {
                return;
            }
            nav.section_states[focus].select(Some(0));
            nav.focus_section = prev_section;
            let prev_len = nav.sections[prev_section].items.len();
            nav.section_states[prev_section].select(Some(prev_len - 1));
        }
        self.emit_nav_select();
    }

    /// Move the navigation focus to the next item, wrapping across sections.
    pub(crate) fn nav_select_next(&mut self) {
        let nav = &mut self.state.navigation.nav;
        if nav.sections.is_empty() {
            return;
        }
        let focus = nav.focus_section;
        let len = nav.sections[focus].items.len();
        if len == 0 {
            return;
        }
        let current = nav.section_states[focus].selected().unwrap_or(0);
        if current + 1 < len {
            nav.section_states[focus].select(Some(current + 1));
        } else {
            let next_section = (focus + 1) % nav.sections.len();
            if nav.sections[next_section].items.is_empty() {
                return;
            }
            nav.section_states[focus].select(Some(0));
            nav.focus_section = next_section;
            nav.section_states[next_section].select(Some(0));
        }
        self.emit_nav_select();
    }

    /// Ask the app to load the endpoint of the focused nav item.
    fn emit_nav_select(&mut self) {
        let nav = &self.state.navigation.nav;
        if nav.sections.is_empty() {
            return;
        }
        let focus = nav.focus_section;
        if let Some(selected) = nav.section_states[focus].selected()
            && let Some(api) = nav.sections[focus]
                .items
                .get(selected)
                .and_then(|i| i.api.as_ref())
        {
            self.events.send(NavigationEvent::NavSelect(api.clone()));
        }
    }
}
