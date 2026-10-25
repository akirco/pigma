use super::{App, event::send_event};
use crate::{
    event::{NavigationEvent, PlaybackEvent},
    playback::{NCM_SEARCH_QUEUE_KEY, THIRD_PARTY_QUEUE_KEY},
    service::ApiEndpoint,
    state::{ContentState, PaginationInfo},
    utils::lock_recover,
};
use ncm_api::SongList;

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
                // Hand the blocking writer a *clone* of the merged content. For
                // `Songs` this is just a cheap `Vec<Arc<_>>` copy — no deep
                // clone, and no shared `Arc` outlives this function, so the next
                // `Arc::make_mut` is not forced into a deep clone. Serialization
                // and the disk write then happen off-thread, and a burst of
                // pages coalesces into one write of the final state
                // (`queue_content_cache`).
                let merged = self.state.navigation.content.as_ref().clone();
                cache.queue_content_cache(&pg_for_save.api, merged, pg_for_save.clone());
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

        let selected_endpoint = self.selected_endpoint();

        let is_album = selected_endpoint == Some(ApiEndpoint::SavedAlbums);
        let is_radio = selected_endpoint == Some(ApiEndpoint::UserRadioSublist);

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
        *lock_recover(&self.playback.state.cover.protocol) = None;
        self.spawn_cover_load(song.id, song.pic_url.clone());
    }

    /// Resolve an endpoint string into playable songs and load them into the
    /// queue without starting playback. Shared by the daemon bootstrap
    /// (`--daemon`) and the IPC `pigma msg switch-list` action. Returns whether
    /// songs were loaded.
    pub(super) async fn load_endpoint(
        &mut self,
        api_str: &str,
        playlist_index: Option<usize>,
    ) -> bool {
        let api = ApiEndpoint::parse(api_str).unwrap_or(ApiEndpoint::RecommendSongs);
        let uid = self.state.navigation.user.as_ref().map(|u| u.uid);
        let content = self
            .service
            .resolve_endpoint_content(api, uid, self.config.search_limit)
            .await;

        // Playlist/toplist endpoints resolve to a *list* of playlists; pick one
        // (default first, or `--playlist N`) and load its songs.
        let content = match content {
            ContentState::SongLists(lists) => {
                self.load_endpoint_list_songs(api, lists, playlist_index)
                    .await
            }
            ContentState::TopLists(lists) => {
                let lists: Vec<SongList> = lists
                    .into_iter()
                    .map(|t| SongList {
                        id: t.id,
                        name: t.name,
                        cover_img_url: t.cover,
                        author: String::new(),
                        subscribed: false,
                    })
                    .collect();
                self.load_endpoint_list_songs(api, lists, playlist_index)
                    .await
            }
            other => other,
        };

        match content {
            ContentState::Songs(songs) if !songs.is_empty() => {
                // Use the nav display name (e.g. " 我喜欢的音乐") as the queue key
                // so the queue title matches the TUI, falling back
                // to the raw endpoint string (e.g. `liked`) when unknown.
                let key = self
                    .config
                    .navigation
                    .name_for_api(api_str)
                    .unwrap_or_else(|| api_str.to_string());
                self.playback.load_songs(&key, songs, 0);
                true
            }
            ContentState::Error(e) => {
                log::error!("endpoint {api_str}: {e}");
                false
            }
            _ => {
                log::warn!("endpoint {api_str} resolved to no playable songs");
                false
            }
        }
    }

    /// Resolve a playlist-list (or toplist) to the songs of the selected
    /// playlist: `--playlist N` picks the 1-based `N`-th entry (default 1st).
    async fn load_endpoint_list_songs(
        &mut self,
        api: ApiEndpoint,
        lists: Vec<SongList>,
        playlist_index: Option<usize>,
    ) -> ContentState {
        if lists.is_empty() {
            return ContentState::Empty;
        }
        let index = playlist_index.unwrap_or(1).max(1) - 1;
        let Some(list) = lists.get(index) else {
            return ContentState::Error(format!(
                "playlist index {} out of range (1..={})",
                index + 1,
                lists.len()
            ));
        };
        self.playback.set_playlist_id(list.id);
        let limit = self.config.search_limit;
        match api {
            ApiEndpoint::SavedAlbums => self.service.load_album(list.id).await,
            ApiEndpoint::UserRadioSublist => {
                let (state, _, _) = self
                    .service
                    .load_playlist_detail(list.id, true, limit)
                    .await;
                state
            }
            _ => {
                let (state, _, _) = self
                    .service
                    .load_playlist_detail(list.id, false, limit)
                    .await;
                state
            }
        }
    }
}
