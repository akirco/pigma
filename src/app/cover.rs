//! Album-cover pipeline: cache lookup → URL resolution (own pic or provider
//! fallback) → download → decode/mask → apply to the playerbar cover state.

use image::GenericImageView;

use super::App;
use crate::playback::CoverState;

impl App {
    /// Spawn the background cover pipeline for `song_id` (`own_pic` is the
    /// song's own cover URL, empty for sonar songs resolved via fallback
    /// search). Stale loaders are dropped by `apply_cover` when the song
    /// changed while loading.
    pub(super) fn spawn_cover_load(&self, song_id: u64, own_pic: String) {
        let is_sonar = sonar::is_sonar_song_id(song_id);
        if own_pic.is_empty() && !is_sonar {
            return;
        }
        let cover = self.playback.state.cover.clone();
        let picker = self.picker.clone();
        let cache = self.service.cache().clone();
        let cover_http = self.cover_http.clone();
        let finder = self.search.finder.clone();
        let registry = self.search.sonar_songs.clone();

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
                    Some(data) => {
                        tokio::task::spawn_blocking(move || build_cover_protocol(&data, picker))
                            .await
                            .ok()
                            .flatten()
                    }
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
                    .or_else(|| cache.thirdparty_song(song_id));
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

            // Download the cover (async client) and process the image off
            // the runtime. The cache was already checked above; a redundant
            // re-check here would double the disk reads on every miss.
            let protocol = {
                let Ok(resp) = cover_http.get(&small_url).send().await else {
                    return;
                };
                let Ok(bytes) = resp.bytes().await else {
                    return;
                };
                let raw = bytes.to_vec();
                let cache = cache.clone();
                tokio::task::spawn_blocking(move || {
                    cache.save_cover(song_id, &raw);
                    build_cover_protocol(&raw, picker.clone())
                })
                .await
                .ok()
                .flatten()
            };

            let Some(protocol) = protocol else {
                return;
            };

            apply_cover(&cover, song_id, protocol);
        });
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
