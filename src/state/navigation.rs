use std::cell::RefCell;
use std::sync::Arc;

use ncm_api::LoginInfo;
use ratatui::widgets::{ListState, TableState};

use crate::config::NavConfig;

use super::Page;
use super::PaginationInfo;
use super::content::{ContentState, TableMode};
use super::login::LoginState;
use super::search::SearchState;

pub use crate::config::NavSectionConfig as NavSection;

pub struct NavState {
    pub sections: Vec<NavSection>,
    pub section_states: Vec<ListState>,
    pub focus_section: usize,
    pub subtitle: Option<String>,
    /// 顶部导航模式下的水平滚动偏移（单元格）。
    pub scroll_x: u16,
}

impl NavState {
    pub fn from_config(config: &NavConfig) -> Self {
        let sections: Vec<NavSection> = if config.sections.is_empty() {
            NavConfig::default().sections
        } else {
            config.sections.clone()
        };

        let section_states: Vec<ListState> = sections
            .iter()
            .map(|s| {
                let mut state = ListState::default();
                if !s.items.is_empty() {
                    state.select(Some(0));
                }
                state
            })
            .collect();

        Self {
            sections,
            section_states,
            focus_section: 0,
            subtitle: None,
            scroll_x: 0,
        }
    }

    pub fn restore_focus_by_api(&mut self, api: &str) {
        for (s, section) in self.sections.iter().enumerate() {
            if let Some(i) = section
                .items
                .iter()
                .position(|item| item.api.as_deref() == Some(api))
            {
                self.focus_section = s;
                self.section_states[s].select(Some(i));
                break;
            }
        }
    }
}

/// (rendered title, focus_section, selected_index, generation, content item count)
type TitleCache = (String, usize, Option<usize>, u64, usize);

#[derive(Clone)]
pub struct BreadcrumbEntry {
    pub content: Arc<ContentState>,
    pub api: Option<String>,
    pub subtitle: Option<String>,
    pub content_selected: usize,
    pub content_column_selected: usize,
    pub table_mode: TableMode,
    pub table_state: TableState,
}

pub struct NavigationState {
    pub page: Page,
    pub login: LoginState,
    pub user: Option<LoginInfo>,
    pub nav: NavState,
    pub content: Arc<ContentState>,
    pub history: Vec<BreadcrumbEntry>,
    pub content_selected: usize,
    pub content_column_selected: usize,
    pub table_mode: TableMode,
    pub table_state: TableState,
    pub playlist_selected: usize,
    pub search: SearchState,
    pub pagination: Option<PaginationInfo>,
    pub generation: u64,
    /// True when the current `Songs` content is a search result (Enter plays
    /// only the selected song instead of appending the whole list to the queue).
    pub content_is_search: bool,
    /// Cached block title string, keyed by
    /// (focus_section, selected_index, generation, content item count).
    /// The item count is part of the key so incremental (paged) loads that
    /// append to the current content re-render the `{count}` placeholder.
    pub title_cache: RefCell<Option<TitleCache>>,
}

impl NavigationState {
    pub fn set_content(&mut self, content: ContentState) {
        self.content = Arc::new(content);
        self.content_selected = 0;
        self.content_column_selected = 0;
        self.table_mode = TableMode::Row;
        self.table_state = TableState::default();
        self.table_state.select_first();
        self.pagination = None;
        *self.title_cache.borrow_mut() = None;
    }

    pub fn push_breadcrumb(&mut self) {
        let api = self
            .nav
            .section_states
            .get(self.nav.focus_section)
            .and_then(|st| st.selected())
            .and_then(|i| self.nav.sections[self.nav.focus_section].items.get(i))
            .and_then(|item| item.api.clone());
        self.history.push(BreadcrumbEntry {
            content: Arc::clone(&self.content),
            api,
            subtitle: self.nav.subtitle.clone(),
            content_selected: self.content_selected,
            content_column_selected: self.content_column_selected,
            table_mode: self.table_mode,
            table_state: self.table_state,
        });
    }

    pub fn pop_breadcrumb(&mut self) -> bool {
        if let Some(entry) = self.history.pop() {
            self.content = entry.content;
            self.content_selected = entry.content_selected;
            self.content_column_selected = entry.content_column_selected;
            self.table_mode = entry.table_mode;
            self.table_state = entry.table_state;
            self.nav.subtitle = entry.subtitle;
            if let Some(api) = &entry.api {
                self.nav.restore_focus_by_api(api);
            }
            *self.title_cache.borrow_mut() = None;
            true
        } else {
            false
        }
    }

    pub fn clear_breadcrumb(&mut self) {
        self.history.clear();
    }
}
