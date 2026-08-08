use std::sync::Arc;

use super::{App, send_event};
use crate::event::{NavigationEvent, PlaybackEvent};
use crate::playback::{CoverState, NCM_SEARCH_QUEUE_KEY, THIRD_PARTY_QUEUE_KEY, parse_lyric_lines};
use crate::state::{ContentState, PaginationInfo};
use image::GenericImageView;

impl App {
    pub(super) fn handle_content_loaded(&mut self, content: ContentState) {
        self.state.navigation.set_content(content);
    }

    pub(super) fn handle_load_more(&mut self) {
        let pg = match self.state.navigation.pagination {
            Some(ref pg) if pg.has_more => pg.clone(),
            _ => return,
        };

        let service = self.service.clone();
        let sender = self.state.events.sender();
        let offset = pg.offset + pg.limit;
        let generation = self.state.navigation.generation;

        tokio::spawn(async move {
            match service.load_more(&pg.api, offset, pg.limit).await {
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
        // 过期响应直接丢弃
        if generation != 0 && generation != self.state.navigation.generation {
            return;
        }

        let same_api =
            self.state.navigation.pagination.as_ref().map(|p| &p.api) == Some(&pagination.api);

        let mut content = content;

        // 仅歌曲列表（云盘、歌单内歌曲）支持分页追加；其余类型整体替换。
        if same_api
            && let ContentState::Songs(new_songs) = &mut content
            && let ContentState::Songs(existing) =
                std::sync::Arc::make_mut(&mut self.state.navigation.content)
        {
            existing.extend(std::mem::take(new_songs));
            let api_str = pagination.api.clone();
            let pg_for_save = pagination.clone();
            self.state.navigation.pagination = Some(pagination);

            let ttl = self.config.cache.content_cache_ttl;
            if ttl > 0 && !api_str.is_empty() {
                let cache = self.service.cache().clone();
                let content_arc = Arc::clone(&self.state.navigation.content);
                tokio::task::spawn_blocking(move || {
                    cache.save_content_cache(&api_str, &content_arc, Some(&pg_for_save));
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

        let selected_api = self
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
            .and_then(|item| item.api.as_deref());

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
        let name = match self.state.navigation.content.as_ref() {
            ContentState::Songs(songs) => songs
                .iter()
                .position(|s| s.id == id)
                .map(|pos| (pos, songs[pos].name.clone())),
            _ => None,
        };
        if let Some((pos, name)) = name {
            if let ContentState::Songs(songs) = self.state.navigation.content.as_ref() {
                if self.state.navigation.content_is_search && sonar::is_sonar_song_id(id) {
                    // 第三方搜索统一进同一个队列，不复用按关键词/日期建的队列
                    self.playback
                        .append_and_play_key(THIRD_PARTY_QUEUE_KEY, &songs[pos..=pos], 0);
                } else if self.state.navigation.content_is_search {
                    // 网易云搜索统一进"官方搜索"队列
                    self.playback
                        .append_and_play_key(NCM_SEARCH_QUEUE_KEY, &songs[pos..=pos], 0);
                } else {
                    let key = self.current_queue_key();
                    self.playback.play_songs(&key, songs.to_vec(), pos);
                }
            }
            self.toast(format!("▶  {}", name));
        }
    }

    pub(super) fn handle_playback_started(&mut self) {
        self.playback.on_playback_started();

        if let Some(song) = self.playback.current_song() {
            if let ContentState::Songs(songs) = self.state.navigation.content.as_ref()
                && let Some(pos) = songs.iter().position(|s| s.id == song.id)
            {
                self.state.navigation.content_selected = pos;
            }
            self.toast(format!("▶  {}", song.name));
            let song_id = song.id;

            if sonar::is_sonar_song_id(song_id) {
                let finder = self.finder.clone();
                let registry = self.sonar_songs.clone();
                let cache = self.service.cache().clone();
                let sender = self.state.events.sender();
                tokio::spawn(async move {
                    // Serve cached lyrics first (keyed by song id, shared with
                    // the NCM path) so replays don't hit the provider again.
                    if let Some(lyrics) = cache.load_lyrics_cache_async(song_id).await {
                        let lyric_lines = parse_lyric_lines(&lyrics.lyric);
                        let tlyric_lines = parse_lyric_lines(&lyrics.tlyric);
                        if !lyric_lines.is_empty() {
                            send_event(
                                &sender,
                                PlaybackEvent::LyricsLoaded {
                                    song_id,
                                    lyrics: lyric_lines,
                                    translated_lyrics: tlyric_lines,
                                }
                                .into(),
                            );
                            return;
                        }
                    }
                    let msong = registry
                        .lock()
                        .ok()
                        .and_then(|m| m.get(&song_id).cloned())
                        .or_else(|| cache.thirdparty_song(song_id).map(Arc::new));
                    let Some(msong) = msong else {
                        return;
                    };
                    let lrc = match finder.get_lyrics_fallback(&msong).await {
                        Some(l) => l,
                        None => return,
                    };
                    let lines: Vec<String> = lrc.lines().map(|s| s.to_string()).collect();
                    let lyric_lines = parse_lyric_lines(&lines);
                    if lyric_lines.is_empty() {
                        return;
                    }
                    cache.save_lyrics_cache(
                        song_id,
                        &ncm_api::Lyrics {
                            lyric: lines,
                            tlyric: Vec::new(),
                        },
                    );
                    send_event(
                        &sender,
                        PlaybackEvent::LyricsLoaded {
                            song_id,
                            lyrics: lyric_lines,
                            translated_lyrics: Vec::new(),
                        }
                        .into(),
                    );
                });
            } else {
                let service = self.service.clone();
                let sender = self.state.events.sender();
                tokio::spawn(async move {
                    if let Some(lyrics) = service.load_lyrics(song_id).await {
                        let lyric_lines = parse_lyric_lines(&lyrics.lyric);
                        let tlyric_lines = parse_lyric_lines(&lyrics.tlyric);
                        send_event(
                            &sender,
                            PlaybackEvent::LyricsLoaded {
                                song_id,
                                lyrics: lyric_lines,
                                translated_lyrics: tlyric_lines,
                            }
                            .into(),
                        );
                    }
                });
            }

            // Clear the cover so a new song never shows the previous one's
            // cover while its own cover is loading (or missing).
            if let Ok(mut guard) = self.playback.state.cover.protocol.lock() {
                *guard = None;
            }

            // Load cover image
            let song_id = song.id;
            let is_sonar = sonar::is_sonar_song_id(song_id);
            let own_pic = song.pic_url.clone();
            let cover = self.playback.state.cover.clone();
            let picker = self.picker.clone();
            let cache = self.service.cache().clone();
            let cover_http = self.cover_http.clone();

            if !own_pic.is_empty() || is_sonar {
                let finder = self.finder.clone();
                let registry = self.sonar_songs.clone();
                tokio::spawn(async move {
                    // Mark whose cover we are loading; a stale loader for a
                    // previously played song will be dropped below.
                    if let Ok(mut g) = cover.song_id.lock() {
                        *g = Some(song_id);
                    }

                    // Serve from cache first — never block a cached cover on
                    // re-resolving the source URL, which can fail offline or
                    // when the third-party provider is unreachable.
                    let cached = {
                        let cache = cache.clone();
                        let picker = picker.clone();
                        match cache.load_cover_async(song_id).await {
                            Some(data) => tokio::task::spawn_blocking(move || {
                                build_cover_protocol(&data, picker)
                            })
                            .await
                            .ok()
                            .flatten(),
                            None => None,
                        }
                    };
                    if let Some(protocol) = cached {
                        apply_cover(&cover, song_id, protocol);
                        return;
                    }

                    // Cache miss — resolve a cover URL: own cover, else
                    // fallback search (kuwo preferred) for sonar songs without
                    // one.
                    let cover_url = if !own_pic.is_empty() {
                        Some(own_pic)
                    } else {
                        let msong = registry
                            .lock()
                            .ok()
                            .and_then(|m| m.get(&song_id).cloned())
                            .or_else(|| cache.thirdparty_song(song_id).map(Arc::new));
                        match msong {
                            Some(msong) => finder.get_cover_fallback(&msong).await,
                            None => None,
                        }
                    };
                    let Some(cover_url) = cover_url else {
                        return;
                    };

                    let small_url = if cover_url.contains('?') {
                        format!("{}&param=200y200", cover_url)
                    } else {
                        format!("{}?param=200y200", cover_url)
                    };

                    // Download the cover (async client — no blocking runtime
                    // owned by App) and process the image off the runtime.
                    let data: Vec<u8> = if let Some(cached) = cache.load_cover_async(song_id).await
                    {
                        cached
                    } else {
                        let Ok(resp) = cover_http.get(&small_url).send().await else {
                            return;
                        };
                        let Ok(bytes) = resp.bytes().await else {
                            return;
                        };
                        let raw = bytes.to_vec();
                        let cache = cache.clone();
                        let value = raw.clone();
                        tokio::task::spawn_blocking(move || cache.save_cover(song_id, &value))
                            .await
                            .ok();
                        raw
                    };

                    let Some(protocol) =
                        tokio::task::spawn_blocking(move || build_cover_protocol(&data, picker))
                            .await
                            .ok()
                            .flatten()
                    else {
                        return;
                    };

                    apply_cover(&cover, song_id, protocol);
                });
            }
        }
    }
}

/// Decode cover bytes, apply the circular mask, and build the resize protocol
/// used by the playerbar renderer.
fn build_cover_protocol(
    data: &[u8],
    picker: ratatui_image::picker::Picker,
) -> Option<ratatui_image::protocol::StatefulProtocol> {
    let Ok(img) = image::load_from_memory(data) else {
        return None;
    };
    let (w, h) = img.dimensions();
    let size = w.min(h);
    let x = (w - size) / 2;
    let y = (h - size) / 2;
    let mut square = img.crop_imm(x, y, size, size).to_rgba8();
    drop(img);

    let r = size as f32 / 2.0;
    for (px, py, pixel) in square.enumerate_pixels_mut() {
        let dx = px as f32 + 0.5 - r;
        let dy = py as f32 + 0.5 - r;
        if dx * dx + dy * dy > r * r {
            *pixel = image::Rgba([0u8, 0, 0, 0]);
        }
    }

    let dyn_img = image::DynamicImage::ImageRgba8(square);
    Some(picker.new_resize_protocol(dyn_img))
}

/// Apply a freshly loaded cover protocol, dropping it if the song changed while
/// it was loading (a stale loader must not overwrite a newer cover).
fn apply_cover(
    cover: &CoverState,
    song_id: u64,
    protocol: ratatui_image::protocol::StatefulProtocol,
) {
    let still_current = cover
        .song_id
        .lock()
        .map(|g| *g == Some(song_id))
        .unwrap_or(false);
    if still_current && let Ok(mut guard) = cover.protocol.lock() {
        *guard = Some(protocol);
    }
}
