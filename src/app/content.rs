use super::{App, send_event};
use crate::event::{AppEvent, Event};
use crate::playback::types::parse_lyric_lines;
use crate::state::{ContentState, PaginationInfo, TableMode};

impl App {
    pub(super) fn handle_content_loaded(&mut self, content: ContentState) {
        self.state.navigation.set_content(content);
        self.state.navigation.content_selected = 0;
        self.state.navigation.table_mode = TableMode::Row;
        self.state.navigation.content_column_selected = 0;
    }

    pub(super) fn handle_load_more(&mut self) {
        let pg = match self.state.navigation.pagination {
            Some(ref pg) if pg.has_more => pg.clone(),
            _ => return,
        };

        let api = self.api.clone();
        let sender = self.state.events.sender();
        let offset = pg.offset + pg.limit;

        tokio::spawn(async move {
            match api.user_cloud_disk(offset, pg.limit).await {
                Ok(result) => {
                    let new_pg = PaginationInfo {
                        api: pg.api.clone(),
                        offset,
                        limit: pg.limit,
                        has_more: result.has_more,
                        total: result.count,
                        loading: false,
                    };
                    send_event(
                        &sender,
                        Event::App(AppEvent::ContentLoadedPaged {
                            content: ContentState::Songs(result.songs),
                            pagination: new_pg,
                        }),
                    );
                }
                Err(e) => {
                    log::error!("Failed to load more cloud disk songs: {e}");
                }
            }
        });
    }

    pub(super) fn handle_content_loaded_paged(
        &mut self,
        content: ContentState,
        pagination: PaginationInfo,
    ) {
        if let (ContentState::Songs(new_songs), ContentState::Songs(existing)) =
            (&content, std::sync::Arc::make_mut(&mut self.state.navigation.content))
        {
            existing.extend(new_songs.iter().cloned());
            self.state.navigation.pagination = Some(pagination.clone());
            *self.state.navigation.content_rows_cache.borrow_mut() = None;

            let ttl = self.config.content_cache_ttl;
            if ttl > 0 && !pagination.api.is_empty() {
                let cache = self.playback.cache().clone();
                let api_str = pagination.api.clone();
                let content_clone = (*self.state.navigation.content).clone();
                tokio::task::spawn_blocking(move || {
                    cache.save_content_cache(&api_str, content_clone);
                });
            }
        } else {
            self.state.navigation.set_content(content);
            self.state.navigation.content_selected = 0;
            self.state.navigation.table_mode = TableMode::Row;
            self.state.navigation.content_column_selected = 0;
            self.state.navigation.pagination = Some(pagination);
        }
    }

    pub(super) fn handle_playlist_select(&mut self, id: u64, name: Option<String>) {
        self.playback.set_playlist_id(id);
        self.state.navigation.push_breadcrumb();
        self.state.navigation.set_content(ContentState::Loading);

        let is_radio = self
            .state
            .navigation
            .nav
            .section_states
            .get(self.state.navigation.nav.focus_section)
            .and_then(|st| st.selected())
            .and_then(|i| {
                self.state.navigation.nav.sections[self.state.navigation.nav.focus_section]
                    .items
                    .get(i)
            })
            .and_then(|item| item.api.as_deref())
            == Some("user_radio_sublist");

        let api = self.api.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            let (state, detail_name) = if is_radio {
                match api.radio_program(id, 0, 1000).await {
                    Ok(songs) => (ContentState::Songs(songs), None),
                    Err(e) => (ContentState::Error(e.to_string()), None),
                }
            } else {
                match api.song_list_detail(id).await {
                    Ok(detail) => (ContentState::Songs(detail.songs), Some(detail.name)),
                    Err(e) => (ContentState::Error(e.to_string()), None),
                }
            };
            send_event(&sender, Event::App(AppEvent::ContentLoaded(state)));
            let breadcrumb = detail_name.or(name);
            if let Some(name) = breadcrumb {
                send_event(&sender, Event::App(AppEvent::BreadcrumbSet(name)));
            }
        });
    }

    pub(super) fn handle_song_play(&mut self, id: u64) {
        if self.playback.is_currently_playing(id) {
            self.playback.toggle_pause();
            self.report_pending_play();
            return;
        }
        let name = match self.state.navigation.content.as_ref() {
            ContentState::Songs(songs) => songs
                .iter()
                .position(|s| s.id == id)
                .map(|pos| (pos, songs[pos].name.clone())),
            _ => None,
        };
        if let Some((pos, name)) = name {
            if let ContentState::Songs(songs) = self.state.navigation.content.as_ref() {
                self.playback.append_and_play(songs, pos);
            }
            self.report_pending_play();
            self.toast(format!("▶  {}", name));
        }
    }

    pub(super) fn handle_playback_started(&mut self) {
        self.playback.on_playback_started();

        if let Some(song) = self.playback.current_song() {
            self.toast(format!("▶  {}", song.name));
            let song_id = song.id;
            let cache = self.playback.cache().clone();
            let api = self.api.clone();
            let sender = self.state.events.sender();

            if let Some(cached) = cache.load_lyrics_cache(song_id) {
                let lyric_lines = parse_lyric_lines(&cached.lyric);
                let tlyric_lines = parse_lyric_lines(&cached.tlyric);
                send_event(
                    &sender,
                    Event::App(AppEvent::LyricsLoaded {
                        song_id,
                        lyrics: lyric_lines,
                        translated_lyrics: tlyric_lines,
                    }),
                );
                return;
            }

            tokio::spawn(async move {
                match api.song_lyric(song_id).await {
                    Ok(lyrics) => {
                        let cache_clone = cache.clone();
                        let lyrics_clone = lyrics.clone();
                        tokio::task::spawn_blocking(move || {
                            cache_clone.save_lyrics_cache(song_id, &lyrics_clone);
                        })
                        .await
                        .ok();
                        let lyric_lines = parse_lyric_lines(&lyrics.lyric);
                        let tlyric_lines = parse_lyric_lines(&lyrics.tlyric);
                        send_event(
                            &sender,
                            Event::App(AppEvent::LyricsLoaded {
                                song_id,
                                lyrics: lyric_lines,
                                translated_lyrics: tlyric_lines,
                            }),
                        );
                    }
                    Err(e) => {
                        log::error!("Failed to fetch lyrics for {song_id}: {e}");
                    }
                }
            });
        }
    }
}
