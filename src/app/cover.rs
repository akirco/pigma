//! Album-cover pipeline: cache lookup → URL resolution (own pic or provider
//! fallback) → download → decode/mask → apply to the playerbar cover state.

use image::GenericImageView;

use super::App;
use crate::{playback::CoverState, utils::lock_recover};

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
            *lock_recover(&cover.song_id) = Some(song_id);

            // Serve from cache first — never block a cached cover on
            // re-resolving the source URL, which can fail offline or
            // when the third-party provider is unreachable.
            let cached = {
                let cache = cache.clone();
                let picker = picker.clone();
                match cache.load_cover_async(song_id).await {
                    Some(data) => {
                        let cache = cache.clone();
                        tokio::task::spawn_blocking(move || {
                            let prepared = build_cover_protocol(&data, picker)?;
                            // Legacy cache entries are the original full-size
                            // covers. Replace them with the small JPEG so future
                            // loads never decode a 4K image at all.
                            if let Some(thumb) = &prepared.thumbnail
                                && data.len() > 64 * 1024
                                && thumb.len() < data.len()
                            {
                                cache.save_cover(song_id, thumb);
                            }
                            Some(prepared.protocol)
                        })
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
                let msong = lock_recover(&registry)
                    .get(&song_id)
                    .cloned()
                    .or_else(|| cache.thirdparty_song(song_id));
                match msong {
                    Some(msong) => finder.get_cover_fallback(&msong).await,
                    None => None,
                }
            };
            let Some(cover_url) = cover_url else {
                return;
            };

            let small_url = sized_cover_url(&cover_url, COVER_MAX_PX);

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
                    let prepared = build_cover_protocol(&raw, picker.clone())?;
                    if let Some(thumb) = &prepared.thumbnail {
                        cache.save_cover(song_id, thumb);
                    }
                    Some(prepared.protocol)
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

/// Longest side a cover is decoded at. The playerbar cover is only a handful of
/// terminal cells, and third-party URLs (Bilibili) ignore NetEase's
/// `?param=200y200`, so a 3240x2025 cover would otherwise be decoded at full
/// resolution for a ~64x64 px target.
const COVER_MAX_PX: u32 = 200;

/// A decoded cover plus its small JPEG counterpart for the disk cache.
struct PreparedCover {
    protocol: ratatui_image::protocol::StatefulProtocol,
    /// `None` when re-encoding failed; the cover still renders, it just is not
    /// written back to the cache.
    thumbnail: Option<Vec<u8>>,
}

/// Decode cover bytes, downsample, apply the circular mask, build the resize
/// protocol, and re-encode a small JPEG for the cache.
fn build_cover_protocol(
    data: &[u8],
    picker: ratatui_image::picker::Picker,
) -> Option<PreparedCover> {
    let prepared = build_cover_protocol_inner(data, picker);
    // The inner call has dropped the decoded buffers by now. glibc keeps freed
    // pages on its free lists, so return them to the OS instead of leaving RSS
    // at the decode peak for the rest of the session.
    crate::utils::process::trim_heap();
    prepared
}

fn build_cover_protocol_inner(
    data: &[u8],
    picker: ratatui_image::picker::Picker,
) -> Option<PreparedCover> {
    let Ok(img) = image::load_from_memory(data) else {
        return None;
    };
    // Downscale before crop + `to_rgba8`: doing those on a 3240x2025 image holds
    // the decoded RGB, the cropped RGB and the RGBA copy simultaneously (~48 MB).
    let img = downscale_cover(img);
    let (w, h) = img.dimensions();
    let size = w.min(h);
    let x = (w - size) / 2;
    let y = (h - size) / 2;
    let mut square = img.crop_imm(x, y, size, size).to_rgba8();

    // JPEG has no alpha, so encode before masking; the mask is reapplied when
    // the cached copy is loaded again.
    let thumbnail = encode_cover_jpeg(&square);

    let r = size as f32 / 2.0;
    for (px, py, pixel) in square.enumerate_pixels_mut() {
        let dx = px as f32 + 0.5 - r;
        let dy = py as f32 + 0.5 - r;
        if dx * dx + dy * dy > r * r {
            *pixel = image::Rgba([0u8, 0, 0, 0]);
        }
    }

    let dyn_img = image::DynamicImage::ImageRgba8(square);
    Some(PreparedCover {
        protocol: picker.new_resize_protocol(dyn_img),
        thumbnail,
    })
}

/// Encode the square cover as a small JPEG for the disk cache.
fn encode_cover_jpeg(img: &image::RgbaImage) -> Option<Vec<u8>> {
    let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
    encoder.encode_image(&rgb).ok()?;
    Some(out)
}

/// Ask the cover CDN for a small image where supported.
///
/// NetEase/Kuwo accept `?param=NxN`, but Bilibili ignores it and uses an
/// `@Nw_Nh_1c.jpg` path suffix instead (`1c` = crop). Keep `.jpg`: the `image`
/// crate is built with the JPEG decoder only.
fn sized_cover_url(url: &str, px: u32) -> String {
    let is_bilibili = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| host == "hdslb.com" || host.ends_with(".hdslb.com"));
    if is_bilibili {
        let (base, query) = url
            .split_once('?')
            .map_or((url, None), |(base, query)| (base, Some(query)));
        // Drop an existing `@...` size suffix so repeated calls stay stable.
        let base = base.split('@').next().unwrap_or(base);
        let sized = format!("{base}@{px}w_{px}h_1c.jpg");
        return match query {
            Some(query) => format!("{sized}?{query}"),
            None => sized,
        };
    }

    if url.contains('?') {
        format!("{url}&param={px}y{px}")
    } else {
        format!("{url}?param={px}y{px}")
    }
}

/// Fit a cover inside [`COVER_MAX_PX`] without upscaling small images.
fn downscale_cover(img: image::DynamicImage) -> image::DynamicImage {
    if img.width() > COVER_MAX_PX || img.height() > COVER_MAX_PX {
        img.thumbnail(COVER_MAX_PX, COVER_MAX_PX)
    } else {
        img
    }
}

/// Apply a freshly loaded cover protocol, dropping it if the song changed while
/// it was loading (a stale loader must not overwrite a newer cover).
fn apply_cover(
    cover: &CoverState,
    song_id: u64,
    protocol: ratatui_image::protocol::StatefulProtocol,
) {
    let still_current = *lock_recover(&cover.song_id) == Some(song_id);
    if still_current {
        *lock_recover(&cover.protocol) = Some(protocol);
    }
}

#[cfg(test)]
mod tests {
    use super::{COVER_MAX_PX, downscale_cover, sized_cover_url};

    #[test]
    fn large_covers_are_downscaled_before_cropping() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(400, 300));
        let img = downscale_cover(img);
        assert!(img.width() <= COVER_MAX_PX && img.height() <= COVER_MAX_PX);
        assert_eq!((img.width(), img.height()), (200, 150));
    }

    #[test]
    fn small_covers_are_not_upscaled() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(100, 80));
        let img = downscale_cover(img);
        assert_eq!((img.width(), img.height()), (100, 80));
    }

    #[test]
    fn bilibili_covers_use_the_path_suffix() {
        let base = "https://i1.hdslb.com/bfs/archive/abc.jpg";
        assert_eq!(
            sized_cover_url(base, 200),
            "https://i1.hdslb.com/bfs/archive/abc.jpg@200w_200h_1c.jpg"
        );
        assert_eq!(
            sized_cover_url(
                "https://i1.hdslb.com/bfs/archive/abc.jpg@480w_270h_1c.webp",
                200
            ),
            "https://i1.hdslb.com/bfs/archive/abc.jpg@200w_200h_1c.jpg"
        );
        assert_eq!(
            sized_cover_url("https://i1.hdslb.com/bfs/archive/abc.jpg?x=1", 200),
            "https://i1.hdslb.com/bfs/archive/abc.jpg@200w_200h_1c.jpg?x=1"
        );
    }

    #[test]
    fn netease_covers_keep_the_param_query() {
        assert_eq!(
            sized_cover_url("https://p1.music.126.net/x.jpg", 200),
            "https://p1.music.126.net/x.jpg?param=200y200"
        );
        assert_eq!(
            sized_cover_url("https://p1.music.126.net/x.jpg?a=1", 200),
            "https://p1.music.126.net/x.jpg?a=1&param=200y200"
        );
    }
}
