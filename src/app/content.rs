use std::sync::Arc;

use super::{App, send_event};
use crate::event::{NavigationEvent, PlaybackEvent};
use crate::playback::types::parse_lyric_lines;
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
            if let Some((content, pagination)) =
                service.load_more_cloud_disk(offset, pg.limit, pg.api).await
            {
                send_event(
                    &sender,
                    NavigationEvent::ContentLoadedPaged {
                        content,
                        pagination,
                        generation,
                    }
                    .into(),
                );
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

        if same_api
            && let (ContentState::Songs(new_songs), ContentState::Songs(existing)) = (
                &content,
                std::sync::Arc::make_mut(&mut self.state.navigation.content),
            )
        {
            existing.extend(new_songs.iter().cloned());
            self.state.navigation.pagination = Some(pagination.clone());
            *self.state.navigation.content_rows_cache.borrow_mut() = None;

            let ttl = self.config.cache.content_cache_ttl;
            if ttl > 0 && !pagination.api.is_empty() {
                let cache = self.service.cache().clone();
                let api_str = pagination.api.clone();
                let content_arc = Arc::clone(&self.state.navigation.content);
                tokio::task::spawn_blocking(move || {
                    cache.save_content_cache(&api_str, &content_arc);
                });
            }
        } else {
            self.state.navigation.set_content(content);
            self.state.navigation.pagination = Some(pagination);
        }
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
        tokio::spawn(async move {
            let (state, detail_name) = if is_album {
                (service.load_album(id).await, None)
            } else {
                service.load_playlist_detail(id, is_radio).await
            };
            send_event(&sender, NavigationEvent::ContentLoaded(state).into());
            let breadcrumb = detail_name.or(name);
            if let Some(name) = breadcrumb {
                send_event(&sender, NavigationEvent::BreadcrumbSet(name).into());
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
                self.playback.append_and_play(songs, pos);
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

            // Load cover image
            if !song.pic_url.is_empty() {
                let song_id = song.id;
                let pic_url = song.pic_url.clone();
                let cover = self.playback.state.cover.0.clone();
                let picker = self.picker.clone();
                let cache = self.service.cache().clone();
                tokio::task::spawn_blocking(move || {
                    let data: Vec<u8> = if let Some(cached) = cache.load_cover(song_id) {
                        cached
                    } else {
                        let small_url = if pic_url.contains('?') {
                            format!("{}&param=200y200", pic_url)
                        } else {
                            format!("{}?param=200y200", pic_url)
                        };

                        let Ok(resp) = reqwest::blocking::get(&small_url) else {
                            return;
                        };
                        let Ok(bytes) = resp.bytes() else {
                            return;
                        };
                        let raw = bytes.to_vec();
                        cache.save_cover(song_id, &raw);
                        raw
                    };

                    let Ok(img) = image::load_from_memory(&data) else {
                        return;
                    };

                    // Apply circular mask
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
                    let protocol = picker.new_resize_protocol(dyn_img);
                    if let Ok(mut guard) = cover.lock() {
                        *guard = Some(protocol);
                    }
                });
            }
        }
    }
}
