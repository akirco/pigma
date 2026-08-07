use std::path::PathBuf;

use super::{App, send_event};
use crate::api::ApiEndpoint;
use crate::event::NavigationEvent;
use crate::state::ContentState;

impl App {
    /// 重新加载当前导航项对应的内容。
    ///
    /// `force = true` 时跳过内容缓存、直接走 API 重新拉取并回写缓存（手动刷新）。
    pub(super) fn handle_nav_select(
        &mut self,
        api_str: String,
        force: bool,
    ) -> color_eyre::Result<()> {
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
                let songs = tokio::task::spawn_blocking(move || {
                    crate::playback::scan_local_music(&music_dir)
                })
                .await
                .unwrap_or_default();
                let state = ContentState::Songs(songs);
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
                    NavigationEvent::ContentLoaded(ContentState::Songs(songs)).into(),
                );
                return;
            }

            if ttl > 0
                && !force
                && api != ApiEndpoint::Search
                && let Some((cached, pg)) = cache.load_content_cache_async(&api_str, ttl).await
            {
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

            // Handle LikedSongs separately: also fetch playlist ID for heartbeat mode
            if api == ApiEndpoint::LikedSongs
                && let Some(uid) = uid
            {
                let (state, playlist_id) = service.load_liked_songs(uid, limit).await;
                let state = if ttl > 0 && !matches!(state, ContentState::Error(_)) {
                    let cache_clone = cache.clone();
                    tokio::task::spawn_blocking(move || {
                        cache_clone.save_content_cache("__liked__", &state, None);
                        state
                    })
                    .await
                    .unwrap_or(ContentState::Empty)
                } else {
                    state
                };
                send_event(&sender, NavigationEvent::ContentLoaded(state).into());
                if let Some(id) = playlist_id {
                    send_event(
                        &sender,
                        crate::event::PlaybackEvent::SetPlaylistId(id).into(),
                    );
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
                    send_event(
                        &sender,
                        crate::event::AppEvent::Toast("未找到文件".into()).into(),
                    );
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
                        crate::event::AppEvent::Toast(format!("⬆ 上传成功: {}", result.song_name))
                            .into(),
                    );
                }
                Err(e) => {
                    log::error!("Upload failed for song_id={song_id}: {e}");
                    send_event(
                        &sender,
                        crate::event::AppEvent::Toast(format!("⬆ 上传失败: {e}")).into(),
                    );
                }
            }
        });
    }

    /// 手动刷新当前导航项：跳过缓存重新拉取并回写缓存。
    pub(crate) fn reload_current_nav(&mut self) {
        let api = self
            .state
            .navigation
            .nav
            .sections
            .get(self.state.navigation.nav.focus_section)
            .and_then(|s| {
                let idx = self
                    .state
                    .navigation
                    .nav
                    .section_states
                    .get(self.state.navigation.nav.focus_section)?
                    .selected()?;
                s.items.get(idx)
            })
            .and_then(|item| item.api.clone());

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
}
