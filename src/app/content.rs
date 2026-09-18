use std::sync::Arc;

use super::{App, event::send_event};
use crate::{
    event::{NavigationEvent, PlaybackEvent},
    playback::{NCM_SEARCH_QUEUE_KEY, THIRD_PARTY_QUEUE_KEY},
    state::{ContentState, PaginationInfo},
};

impl App {
    pub(super) fn handle_content_loaded(&mut self, content: ContentState) {
        self.state.navigation.set_content(content);
    }

    pub(super) fn handle_load_more(&mut self) {
        let (api, offset, limit) = match self.state.navigation.pagination.as_ref() {
            Some(pg) if pg.has_more => (pg.api.clone(), pg.next_offset(), pg.limit),
            _ => return,
        };

        let service = self.service.clone();
        let sender = self.state.events.sender();
        let generation = self.state.navigation.generation;

        tokio::spawn(async move {
            match service.load_more(&api, offset, limit).await {
                Some((content, pagination)) => send_event(
                    &sender,
                    NavigationEvent::ContentLoadedPaged {
                        content,
                        pagination,
                        generation,
                    }
                    .into(),
                ),
                // Release the in-flight flag, otherwise pagination stays stuck
                // after a single transient failure.
                None => send_event(&sender, NavigationEvent::LoadMoreFailed.into()),
            }
        });
    }

    pub(super) fn handle_content_loaded_paged(
        &mut self,
        content: ContentState,
        pagination: PaginationInfo,
        generation: u64,
    ) {
        // Drop stale responses
        if generation != 0 && generation != self.state.navigation.generation {
            return;
        }

        let same_api =
            self.state.navigation.pagination.as_ref().map(|p| &p.api) == Some(&pagination.api);

        let mut content = content;

        // Only song lists (cloud disk, songs within a playlist) support paged appends; other types replace the whole content.
        if same_api
            && let ContentState::Songs(new_songs) = &mut content
            && matches!(
                self.state.navigation.content.as_ref(),
                ContentState::Songs(_)
            )
        {
            std::sync::Arc::make_mut(&mut self.state.navigation.content)
                .append_unique_songs(std::mem::take(new_songs));
            let pg_for_save = pagination.clone();
            self.state.navigation.pagination = Some(pagination);

            let ttl = self.config.cache.content_cache_ttl;
            if ttl > 0 && !pg_for_save.api.is_empty() {
                let cache = self.service.cache().clone();
                let content_arc = Arc::clone(&self.state.navigation.content);
                tokio::task::spawn_blocking(move || {
                    cache.save_content_cache(&pg_for_save.api, &content_arc, Some(&pg_for_save));
                });
            }
            return;
        }
        self.state.navigation.set_content(content);
        self.state.navigation.pagination = Some(pagination);
    }

    pub(super) fn handle_playlist_select(&mut self, id: u64, name: Option<String>) {
        self.state.navigation.push_breadcrumb();
        self.state.navigation.set_content(ContentState::Loading);
        // The playlist is being reloaded (content may have changed), so invalidate the previous "全量已入队" marker.
        self.queued_playlists.remove(&id);

        let selected_api = self.state.navigation.nav.selected_api();

        let is_album = selected_api == Some("album_sublist");
        let is_radio = selected_api == Some("user_radio_sublist");

        if !is_album {
            self.playback.set_playlist_id(id);
        }

        let service = self.service.clone();
        let sender = self.state.events.sender();
        let limit = self.config.search_limit;
        tokio::spawn(async move {
            if is_album {
                let state = service.load_album(id).await;
                send_event(&sender, NavigationEvent::ContentLoaded(state).into());
                if let Some(n) = name.clone() {
                    send_event(&sender, NavigationEvent::BreadcrumbSet(n).into());
                }
                return;
            }
            let (state, detail_name, pagination) =
                service.load_playlist_detail(id, is_radio, limit).await;
            if let Some(pg) = pagination {
                send_event(
                    &sender,
                    NavigationEvent::ContentLoadedPaged {
                        content: state,
                        pagination: pg,
                        generation: 0,
                    }
                    .into(),
                );
            } else {
                send_event(&sender, NavigationEvent::ContentLoaded(state).into());
            }
            let breadcrumb = detail_name.or(name);
            if let Some(n) = breadcrumb {
                send_event(&sender, NavigationEvent::BreadcrumbSet(n).into());
            }
        });
    }

    pub(super) fn handle_song_play(&mut self, id: u64) {
        if self.playback.is_currently_playing(id) {
            self.playback.toggle_pause();
            return;
        }
        let pos = match self.state.navigation.content.as_ref() {
            ContentState::Songs(songs) => songs.iter().position(|s| s.id == id),
            _ => None,
        };
        if let Some(pos) = pos {
            if let ContentState::Songs(songs) = self.state.navigation.content.as_ref() {
                if self.state.navigation.content_is_search && sonar::is_sonar_song_id(id) {
                    // Third-party search always goes into the same queue; do not reuse queues built by keyword/date
                    self.playback
                        .append_and_play_key(THIRD_PARTY_QUEUE_KEY, &songs[pos..=pos], 0);
                } else if self.state.navigation.content_is_search {
                    // NetEase Cloud search always goes into the "官方搜索" queue
                    self.playback
                        .append_and_play_key(NCM_SEARCH_QUEUE_KEY, &songs[pos..=pos], 0);
                } else {
                    let key = self.state.navigation.current_queue_key();
                    let lazy_id = self
                        .state
                        .navigation
                        .pagination
                        .as_ref()
                        .filter(|p| p.has_more)
                        .and_then(|p| p.api.strip_prefix("playlist:"))
                        .and_then(|s| s.parse::<u64>().ok());

                    if let Some(id) = lazy_id {
                        if self.queued_playlists.contains(&id) {
                            // The full track list was already merged into this playlist's
                            // queue (in memory or persisted), so activate the queue directly
                            // and seek to the song, avoiding rebuilding/truncating or refetching.
                            // Locate by song ID rather than content-list index: `a` inserts
                            // the next song after the current one, so the queue order no longer
                            // matches the content list, and `play_index` by content index would
                            // play the wrong song.
                            let qkey = self.playback.queue_key_for(&key);
                            self.playback.activate_queue(&qkey);
                            if let Some(qidx) =
                                self.playback.queue_songs().iter().position(|s| s.id == id)
                            {
                                self.playback.play_index(qidx);
                            } else {
                                self.playback.play_songs(&key, songs.to_vec(), pos);
                            }
                        } else {
                            // Lazily-paged playlist: play the first page immediately; the remaining tracks are merged into the same queue in the background in batches.
                            self.playback.play_songs(&key, songs.to_vec(), pos);
                            let (api, limit, total) = {
                                let p = self
                                    .state
                                    .navigation
                                    .pagination
                                    .as_ref()
                                    .expect("lazy branch implies pagination is Some");
                                (p.api.clone(), p.limit, p.total)
                            };
                            let qkey = self.playback.queue_key_for(&key);
                            let service = self.service.clone();
                            let sender = self.state.events.sender();
                            let start = songs.len() as u32;
                            tokio::spawn(async move {
                                let mut offset = start;
                                let mut completed = true;
                                loop {
                                    match service.load_more(&api, offset, limit).await {
                                        Some((ContentState::Songs(page), next_pg)) => {
                                            if page.is_empty() {
                                                break;
                                            }
                                            send_event(
                                                &sender,
                                                PlaybackEvent::QueueAppend {
                                                    key: qkey.clone(),
                                                    songs: page,
                                                }
                                                .into(),
                                            );
                                            offset = next_pg.offset + next_pg.limit;
                                            if !next_pg.has_more || u64::from(offset) >= total {
                                                break;
                                            }
                                        }
                                        _ => {
                                            completed = false;
                                            break;
                                        }
                                    }
                                }
                                if completed {
                                    send_event(
                                        &sender,
                                        PlaybackEvent::QueueLoadDone { playlist_id: id }.into(),
                                    );
                                }
                            });
                        }
                    } else {
                        self.playback.play_songs(&key, songs.to_vec(), pos);
                    }
                }
            }
            let toast_name: &str = match self.state.navigation.content.as_ref() {
                ContentState::Songs(songs) => songs.get(pos).map(|s| s.name.as_str()).unwrap_or(""),
                _ => "",
            };
            self.toast(format!("▶  {}", toast_name));
        }
    }

    pub(super) fn handle_playback_started(&mut self) {
        self.playback.on_playback_started();

        let Some(song) = self.playback.current_song() else {
            return;
        };
        if let ContentState::Songs(songs) = self.state.navigation.content.as_ref()
            && let Some(pos) = songs.iter().position(|s| s.id == song.id)
        {
            self.state.navigation.content_selected = pos;
        }
        self.toast(format!("▶  {}", song.name));

        self.spawn_lyrics_load(song.id);

        // Clear the cover so a new song never shows the previous one's
        // cover while its own cover is loading (or missing).
        if let Ok(mut guard) = self.playback.state.cover.protocol.lock() {
            *guard = None;
        }
        self.spawn_cover_load(song.id, song.pic_url.clone());
    }
}
