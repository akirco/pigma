use std::path::PathBuf;

use super::{App, send_event};
use crate::event::{AppEvent, NavigationEvent, PlaybackEvent};
use crate::playback::scan_local_music;
use crate::service::ApiEndpoint;
use crate::state::{ContentState, Page};

impl App {
    /// Reload the content for the current navigation item.
    ///
    /// With `force = true`, skip the content cache, refetch directly from the API,
    /// and write the result back to the cache (manual refresh).
    pub(super) fn handle_nav_select(
        &mut self,
        api_str: String,
        force: bool,
    ) -> color_eyre::Result<()> {
        if api_str == "login" {
            self.state.navigation.page = Page::Login;
            return Ok(());
        }
        let api = match ApiEndpoint::parse(&api_str) {
            Some(ep) => ep,
            None => {
                self.state
                    .navigation
                    .set_content(ContentState::Error(format!("未知: {api_str}")));
                return Ok(());
            }
        };

        if api == ApiEndpoint::LocalMusic {
            self.state.navigation.content_is_search = false;
            self.state.navigation.clear_breadcrumb();
            self.state.navigation.set_content(ContentState::Loading);
            let cache = self.service.cache().clone();
            let ttl = self.config.cache.content_cache_ttl;
            let sender = self.state.events.sender();
            let music_dir = dirs::home_dir().unwrap_or_default().join("Music");

            tokio::spawn(async move {
                if ttl > 0
                    && let Some((cached, _)) = cache.load_content_cache_async(&api_str, ttl).await
                {
                    send_event(&sender, NavigationEvent::ContentLoaded(cached).into());
                    return;
                }
                let songs = tokio::task::spawn_blocking(move || scan_local_music(&music_dir))
                    .await
                    .unwrap_or_default();
                let state =
                    ContentState::Songs(songs.into_iter().map(std::sync::Arc::new).collect());
                let state = if ttl > 0 {
                    let cache_clone = cache.clone();
                    tokio::task::spawn_blocking(move || {
                        cache_clone.save_content_cache("__local_music__", &state, None);
                        state
                    })
                    .await
                    .unwrap_or(ContentState::Empty)
                } else {
                    state
                };
                send_event(&sender, NavigationEvent::ContentLoaded(state).into());
            });
            return Ok(());
        }

        self.state.navigation.clear_breadcrumb();
        self.state.navigation.content_is_search = false;
        self.state.navigation.set_content(ContentState::Loading);
        self.state.navigation.nav.subtitle = None;
        self.state.navigation.generation += 1;
        let generation = self.state.navigation.generation;
        if api == ApiEndpoint::Search {
            self.state.navigation.nav.subtitle = Some("热搜榜".into());
        }
        let cache = self.service.cache().clone();
        let service = self.service.clone();
        let sender = self.state.events.sender();
        let uid = self.state.navigation.user.as_ref().map(|u| u.uid);
        let ttl = self.config.cache.content_cache_ttl;
        let limit = self.config.search_limit;

        tokio::spawn(async move {
            if api == ApiEndpoint::Download {
                let songs = cache.list_cached_songs_async().await;
                send_event(
                    &sender,
                    NavigationEvent::ContentLoaded(ContentState::Songs(
                        songs.into_iter().map(std::sync::Arc::new).collect(),
                    ))
                    .into(),
                );
                return;
            }

            if ttl > 0
                && !force
                && api != ApiEndpoint::Search
                && let Some((cached, pg)) = cache.load_content_cache_async(&api_str, ttl).await
                && (api != ApiEndpoint::LikedSongs || pg.is_some())
            {
                // When restoring playlist pagination from cache, top up the trackIds, otherwise lazy pagination (LoadMore) cannot slice.
                if let Some(pg) = &pg
                    && let Some(id_str) = pg.api.strip_prefix("playlist:")
                    && let Ok(id) = id_str.parse::<u64>()
                {
                    let service = service.clone();
                    tokio::spawn(async move {
                        if let Some(ids) = service.ensure_playlist_track_ids(id).await {
                            log::debug!("refetched trackIds for playlist {id}: {} ids", ids);
                        }
                    });
                }

                if let Some(pg) = pg {
                    send_event(
                        &sender,
                        NavigationEvent::ContentLoadedPaged {
                            content: cached,
                            pagination: pg,
                            generation,
                        }
                        .into(),
                    );
                } else {
                    send_event(&sender, NavigationEvent::ContentLoaded(cached).into());
                }
                return;
            }

            // Handle LikedSongs separately: also fetch playlist ID for heartbeat mode.
            // Paginate lazily through the "我喜欢的音乐" playlist's trackIds to avoid stalling from fetching everything at once.
            if api == ApiEndpoint::LikedSongs
                && let Some(uid) = uid
            {
                let (state, pagination, playlist_id) = service.load_liked_songs(uid, limit).await;
                let state = if ttl > 0 && !matches!(state, ContentState::Error(_)) {
                    let cache_clone = cache.clone();
                    let pg_for_save = pagination.clone();
                    tokio::task::spawn_blocking(move || {
                        cache_clone.save_content_cache("__liked__", &state, pg_for_save.as_ref());
                        state
                    })
                    .await
                    .unwrap_or(ContentState::Empty)
                } else {
                    state
                };
                if let Some(pg) = pagination {
                    send_event(
                        &sender,
                        NavigationEvent::ContentLoadedPaged {
                            content: state,
                            pagination: pg,
                            generation,
                        }
                        .into(),
                    );
                } else {
                    send_event(&sender, NavigationEvent::ContentLoaded(state).into());
                }
                if let Some(id) = playlist_id {
                    send_event(&sender, PlaybackEvent::SetPlaylistId(id).into());
                }
                return;
            }

            let (state, pagination) = service.resolve_content(api, uid, limit).await;

            let state = if ttl > 0
                && api != ApiEndpoint::Search
                && !matches!(state, ContentState::Error(_))
            {
                let cache_clone = cache.clone();
                let pg_for_save = pagination.clone();
                tokio::task::spawn_blocking(move || {
                    cache_clone.save_content_cache(&api_str, &state, pg_for_save.as_ref());
                    state
                })
                .await
                .unwrap_or(ContentState::Empty)
            } else {
                state
            };

            if let Some(pg) = pagination {
                send_event(
                    &sender,
                    NavigationEvent::ContentLoadedPaged {
                        content: state,
                        pagination: pg,
                        generation,
                    }
                    .into(),
                );
            } else {
                send_event(&sender, NavigationEvent::ContentLoaded(state).into());
            }
        });
        Ok(())
    }

    pub(super) fn handle_breadcrumb(&mut self, name: String) {
        self.state.navigation.nav.subtitle = Some(name);
    }

    pub(super) fn handle_cell_action(&mut self, row: usize, col: usize) -> color_eyre::Result<()> {
        let columns = self
            .config
            .columns
            .for_content(self.state.navigation.content.content_type(), None)
            .to_vec();
        let Some(column) = columns.get(col) else {
            return Ok(());
        };
        let field = column.field.as_str();

        match (self.state.navigation.content.as_ref(), field) {
            (ContentState::Songs(songs), "album") => {
                if let Some(song) = songs.get(row) {
                    let album_id = song.album_id;
                    let name = format!("{}: {}", column.header, song.album);
                    self.state.navigation.push_breadcrumb();
                    self.state.navigation.set_content(ContentState::Loading);
                    let service = self.service.clone();
                    let sender = self.state.events.sender();
                    tokio::spawn(async move {
                        let state = service.load_album(album_id).await;
                        let _ = sender.send(NavigationEvent::ContentLoaded(state).into());
                        let _ = sender.send(NavigationEvent::BreadcrumbSet(name).into());
                    });
                }
            }
            (ContentState::Songs(songs), "singer") => {
                if let Some(song) = songs.get(row) {
                    let artist_id = song.artist_id;
                    if artist_id == 0 {
                        return Ok(());
                    }
                    let name = format!("{}: {}", column.header, song.singer);
                    self.state.navigation.push_breadcrumb();
                    self.state.navigation.set_content(ContentState::Loading);
                    let service = self.service.clone();
                    let sender = self.state.events.sender();
                    let limit = self.config.search_limit;
                    tokio::spawn(async move {
                        let state = service.load_artist_songs(artist_id, limit).await;
                        let _ = sender.send(NavigationEvent::ContentLoaded(state).into());
                        let _ = sender.send(NavigationEvent::BreadcrumbSet(name).into());
                    });
                }
            }
            (ContentState::Singers(singers), "name") => {
                if let Some(singer) = singers.get(row) {
                    let artist_id = singer.id;
                    if artist_id == 0 {
                        return Ok(());
                    }
                    let name = format!("{}: {}", column.header, singer.name);
                    self.state.navigation.push_breadcrumb();
                    self.state.navigation.set_content(ContentState::Loading);
                    let service = self.service.clone();
                    let sender = self.state.events.sender();
                    let limit = self.config.search_limit;
                    tokio::spawn(async move {
                        let state = service.load_artist_songs(artist_id, limit).await;
                        let _ = sender.send(NavigationEvent::ContentLoaded(state).into());
                        let _ = sender.send(NavigationEvent::BreadcrumbSet(name).into());
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn handle_upload_cached_song(&mut self, row: usize) {
        let songs = match self.state.navigation.content.as_ref() {
            ContentState::Songs(songs) => songs,
            _ => return,
        };
        let song = match songs.get(row) {
            Some(s) => s.clone(),
            None => return,
        };

        self.toast(format!("⬆ 正在上传 {}...", song.name));

        let is_local = song.copyright == ncm_api::SongCopyright::Free
            && !song.album.is_empty()
            && std::path::Path::new(&song.album).exists();
        let service = self.service.clone();
        let cache = self.service.cache().clone();
        let sender = self.state.events.sender();
        let song_id = song.id;
        let cached_path: Option<PathBuf> = if is_local {
            Some(std::path::PathBuf::from(&song.album))
        } else {
            const EXTS: &[&str] = &["mp3", "flac", "m4a", "ogg"];
            EXTS.iter().find_map(|ext| {
                let p = cache.cache_path(song_id, ext);
                if p.exists() { Some(p) } else { None }
            })
        };

        tokio::spawn(async move {
            let path = match cached_path {
                Some(p) => p,
                None => {
                    send_event(&sender, AppEvent::Toast("未找到文件".into()).into());
                    return;
                }
            };

            match service
                .upload_song_with_meta(&path, &song.name, &song.album, &song.singer)
                .await
            {
                Ok(result) => {
                    cache.mark_uploaded(song_id);
                    log::info!("Uploaded {} (song_id={})", result.song_name, result.song_id);
                    send_event(
                        &sender,
                        NavigationEvent::NavSelect("__download__".into()).into(),
                    );
                    send_event(
                        &sender,
                        AppEvent::Toast(format!("⬆ 上传成功: {}", result.song_name)).into(),
                    );
                }
                Err(e) => {
                    log::error!("Upload failed for song_id={song_id}: {e}");
                    send_event(&sender, AppEvent::Toast(format!("⬆ 上传失败: {e}")).into());
                }
            }
        });
    }

    /// Manually refresh the current navigation item: skip the cache, refetch, and write the result back to the cache.
    pub(crate) fn reload_current_nav(&mut self) {
        let api = self.state.navigation.nav.selected_api().map(str::to_string);

        match api.as_deref() {
            Some("__local_music__") => self.toast("↻ 刷新本地音乐".into()),
            Some("__download__") => self.toast("↻ 刷新下载".into()),
            Some(api_str) => {
                let _ = self.handle_nav_select(api_str.to_string(), true);
                self.toast("↻ 刷新当前内容".into());
            }
            None => self.toast("无可用内容刷新".into()),
        }
    }

    pub(super) fn navigate_to_local(&mut self) {
        self.state.navigation.page = Page::Main;
        self.state.navigation.nav.focus_section = 1;
        if let Some(s) = self.state.navigation.nav.sections.get(1)
            && let Some(i) = s.items.iter().position(|item| item.name == "本地音乐")
        {
            self.state.navigation.nav.section_states[1].select(Some(i));
        }
        self.state.navigation.nav.subtitle = Some("本地音乐".into());
        let sender = self.state.events.sender();
        send_event(
            &sender,
            NavigationEvent::NavSelect("__local_music__".into()).into(),
        );
        self.state.navigation.content_selected = 0;
    }

    pub(super) fn navigate_to_main(&mut self) {
        self.state.navigation.page = Page::Main;

        // Browsable without a logged-in session (public endpoints).
        const PUBLIC_APIS: &[&str] = &["top_song_list", "toplist", "top_singers", "search"];
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
                    if public_only && !PUBLIC_APIS.contains(&api) {
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
            let sender = self.state.events.sender();
            send_event(&sender, NavigationEvent::NavSelect(api).into());
        }
    }

    /// Breadcrumb key for the current page: the last breadcrumb level's
    /// subtitle, falling back to the focused nav item's name. Distinct pages
    /// get distinct playback queues.
    pub(super) fn current_queue_key(&self) -> String {
        let nav = &self.state.navigation;
        if let Some(sub) = nav.nav.subtitle.as_deref().filter(|s| !s.trim().is_empty()) {
            return sub.to_string();
        }
        nav.nav
            .selected_name()
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "默认队列".into())
    }
}
