use std::{cell::RefCell, ops::Range, sync::Arc};

use ncm_api::{LoginInfo, SongInfo};
use ratatui::{
    layout::Rect,
    widgets::{ListState, TableState},
};

use super::{ContentState, Page, PaginationInfo, content::TableMode, search::SearchState};
use crate::config::NavConfig;
pub use crate::config::{NavItemConfig, NavSectionConfig as NavSection};

pub struct NavState {
    pub sections: Vec<NavSection>,
    pub section_states: Vec<ListState>,
    pub focus_section: usize,
    pub subtitle: Option<String>,
    /// Bordered UI: characters around the selected or hovered item.
    pub selected_left: String,
    pub selected_right: String,
    /// Horizontal scroll offset (in cells) for the top navigation mode.
    pub scroll_x: u16,
    /// Inner rect of the sidebar list in the last frame, used to map mouse
    /// clicks back to nav rows. Zero-sized when the sidebar was not drawn.
    pub list_area: ratatui::layout::Rect,
    /// Vertical scroll offset of the sidebar list in the last frame.
    pub scroll_y: usize,
    /// Inner rect of the horizontal navigation row in the last frame.
    pub top_area: ratatui::layout::Rect,
    /// Clickable items of that row: line columns and the section/item they
    /// select. Empty when the sidebar layout is active.
    pub top_hits: Vec<(std::ops::Range<usize>, usize, usize)>,
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
            selected_left: config.selected_left.clone(),
            selected_right: config.selected_right.clone(),
            scroll_x: 0,
            list_area: ratatui::layout::Rect::default(),
            scroll_y: 0,
            top_area: ratatui::layout::Rect::default(),
            top_hits: Vec::new(),
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

    /// The focused section, if the focus index is in range.
    pub fn focused_section(&self) -> Option<&NavSection> {
        self.sections.get(self.focus_section)
    }

    /// Selected item index within the focused section.
    pub fn selected_index(&self) -> Option<usize> {
        self.section_states
            .get(self.focus_section)
            .and_then(|st| st.selected())
    }

    /// The currently focused and selected nav item, if any.
    pub fn selected_item(&self) -> Option<&NavItemConfig> {
        let section = self.focused_section()?;
        self.selected_index().and_then(|i| section.items.get(i))
    }

    /// The api of the currently focused nav item, if any.
    pub fn selected_api(&self) -> Option<&str> {
        self.selected_item().and_then(|item| item.api.as_deref())
    }

    /// The display name of the currently focused nav item, if any.
    pub fn selected_name(&self) -> Option<&str> {
        self.selected_item().map(|item| item.name.as_str())
    }
}

/// One clickable tab in the queue tab bar: the columns it occupies in the tab
/// line and the queue index it selects.
#[derive(Debug, Clone)]
pub struct QueueTabHit {
    pub columns: std::ops::Range<usize>,
    pub index: usize,
}

/// (rendered title, focus_section, selected_index, generation, content item
/// count, paged total). The total is part of the key so a `count/total` title
/// refreshes when the server reports a new total without adding loaded rows.
type TitleCache = (Arc<String>, usize, Option<usize>, u64, usize, usize);

#[derive(Clone)]
pub struct BreadcrumbEntry {
    pub content: Arc<ContentState>,
    pub api: Option<String>,
    pub subtitle: Option<String>,
    pub content_selected: usize,
    pub content_column_selected: usize,
    pub content_scroll: usize,
    pub table_mode: TableMode,
    pub table_state: TableState,
}

pub struct NavigationState {
    pub page: Page,
    pub user: Option<LoginInfo>,
    pub nav: NavState,
    pub content: Arc<ContentState>,
    pub history: Vec<BreadcrumbEntry>,
    pub content_selected: usize,
    pub content_column_selected: usize,
    /// Layout facts produced by the last frame and applied by
    /// `App::apply_render_outcome` (see `ui::RenderOutcome`). Input handlers run
    /// after `draw`, so these always describe the frame on screen.
    pub content_columns: usize,
    /// Screen-row scroll offset for the card grid.
    pub content_scroll: usize,
    /// Item range drawn last frame, used to prefetch covers.
    pub content_visible: Range<usize>,
    /// Item under the pointer in the last frame, used by mouse activation.
    pub content_hovered: Option<usize>,
    pub table_mode: TableMode,
    pub table_state: TableState,
    pub playlist_selected: usize,
    /// Horizontal scroll offset of the queue's tab bar (persisted across renders
    /// so the selected tab stays in view).
    pub queue_tab_scroll_x: u16,
    /// Queue table body in the last frame and its first visible song, used to
    /// map mouse clicks back to queue rows. Zero-sized when not drawn.
    pub queue_body: Rect,
    pub queue_offset: usize,
    /// Queue tab bar in the last frame and its clickable tabs.
    pub queue_tab_area: Rect,
    pub queue_tab_hits: Vec<QueueTabHit>,
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
    /// This is a frame-local render cache, not business state: only
    /// `ui::title` reads or writes it, via interior mutability.
    pub title_cache: RefCell<Option<TitleCache>>,
}

impl NavigationState {
    /// Keep the cursor inside the loaded content after any mutation. The
    /// renderer used to clamp a private copy, which let the highlighted row and
    /// `content_selected` diverge (Enter then found nothing). Clamp at the
    /// write site instead so every consumer reads the same value.
    pub fn clamp_selection(&mut self) {
        let len = self.content.len();
        let sel = self.content_selected.min(len.saturating_sub(1));
        if sel != self.content_selected {
            self.content_selected = sel;
            self.table_state.select((len > 0).then_some(sel));
        }
    }

    pub fn set_content(&mut self, content: ContentState) {
        self.content = Arc::new(content);
        self.content_selected = 0;
        self.content_column_selected = 0;
        self.content_columns = 1;
        self.content_scroll = 0;
        self.content_visible = 0..0;
        self.table_mode = TableMode::Row;
        self.table_state = TableState::default();
        self.table_state.select_first();
        self.pagination = None;
        *self.title_cache.borrow_mut() = None;
    }

    pub fn push_breadcrumb(&mut self) {
        let api = self.nav.selected_api().map(str::to_string);
        self.history.push(BreadcrumbEntry {
            content: Arc::clone(&self.content),
            api,
            subtitle: self.nav.subtitle.clone(),
            content_selected: self.content_selected,
            content_column_selected: self.content_column_selected,
            content_scroll: self.content_scroll,
            table_mode: self.table_mode,
            table_state: self.table_state,
        });
    }

    pub fn pop_breadcrumb(&mut self) -> bool {
        if let Some(entry) = self.history.pop() {
            self.content = entry.content;
            self.content_selected = entry.content_selected;
            self.content_column_selected = entry.content_column_selected;
            self.content_scroll = entry.content_scroll;
            self.table_mode = entry.table_mode;
            self.table_state = entry.table_state;
            self.nav.subtitle = entry.subtitle;
            if let Some(api) = &entry.api {
                self.nav.restore_focus_by_api(api);
            }
            self.clamp_selection();
            *self.title_cache.borrow_mut() = None;
            true
        } else {
            false
        }
    }

    /// Remove a song from the active content while keeping selection and pagination valid.
    pub fn remove_song(&mut self, song_id: u64) -> bool {
        if !Arc::make_mut(&mut self.content).remove_song(song_id) {
            return false;
        }

        self.clamp_selection();
        if let Some(pagination) = &mut self.pagination {
            pagination.total = pagination.total.saturating_sub(1);
        }
        *self.title_cache.borrow_mut() = None;
        true
    }

    /// Insert a song at the top of the current song content, mirroring a newly
    /// liked song onto the open "我喜欢的音乐" root. Returns whether an item was inserted.
    pub fn insert_song_at_top(&mut self, song: Arc<SongInfo>) -> bool {
        if !Arc::make_mut(&mut self.content).insert_song_at_top(song) {
            return false;
        }
        // Follow the previously selected row, which was shifted down by one.
        self.content_selected = self.content_selected.saturating_add(1);
        if self.table_state.selected().is_some() {
            self.table_state.select(Some(self.content_selected));
        }
        if let Some(pagination) = &mut self.pagination {
            pagination.total = pagination.total.saturating_add(1);
        }
        self.clamp_selection();
        *self.title_cache.borrow_mut() = None;
        true
    }

    pub fn clear_breadcrumb(&mut self) {
        self.history.clear();
    }

    /// Breadcrumb key for the current page: the last breadcrumb level's
    /// subtitle, falling back to the focused nav item's name. Distinct pages
    /// get distinct playback queues.
    pub fn current_queue_key(&self) -> String {
        if let Some(sub) = self
            .nav
            .subtitle
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            return sub.to_string();
        }
        self.nav
            .selected_name()
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "默认队列".into())
    }
}
