//! Album-cover pipeline: cache lookup → URL resolution (own pic or provider
//! fallback) → download → decode/mask → apply to the playerbar cover state,
//! plus the in-memory thumbnails used by the content cards.

mod cache;

use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use image::GenericImageView;
use ratatui_image::protocol::StatefulProtocol;

use cache::CoverCache;

use super::App;
use crate::{
    config::{ContentStyle, ContentType},
    event::AppEvent,
    lock::lock_recover,
    playback::CoverState,
    state::ContentState,
};

/// Minimum gap between coalesced redraws from one prefetch batch.
const REDRAW_MIN_INTERVAL: Duration = Duration::from_millis(150);

/// Covers fetched before/after the visible window. One screen of slack lets a
/// scroll step render without waiting for a request.
const PREFETCH_MARGIN_MIN: usize = 8;

/// Lists up to this length are prefetched whole. A couple of screens of covers
/// is cheap and keeps the cache independent of the renderer's visible range.
const PREFETCH_WHOLE_LIST_MAX: usize = 60;

/// Rows prefetched on each side of the cursor in long lists.
const PREFETCH_ROWS: usize = 3;

/// Cover thumbnails for the content cards, keyed by the original cover URL.
pub(crate) type ContentCovers = Arc<Mutex<CoverCache>>;

impl App {
    /// Fetch and decode thumbnails for the current content cards. Results stay
    /// in the bounded `ContentCovers` cache for the session; `cover_requests`
    /// keeps in-flight URLs from being requested twice by consecutive content
    /// loads.
    pub(crate) fn prefetch_content_covers(&mut self) {
        if self
            .config
            .content
            .style_for(self.state.navigation.content.content_type())
            != ContentStyle::Cards
        {
            return;
        }
        let content = self.state.navigation.content.as_ref();
        let total = content.len();
        // Singer portraits are shown as circles; every other content type keeps
        // its square cover.
        let round = content.content_type() == ContentType::Singers;
        let px = if round {
            SINGER_COVER_MAX_PX
        } else {
            CONTENT_COVER_MAX_PX
        };
        // Prefetch the visible window plus a margin. Fetching a short list in
        // full used to look harmless, but the thumbnail cache is session-wide:
        // every visited list added its covers on top of the previous ones. A
        // window keeps both requests and retained thumbnails proportional to
        // what the user can actually see.
        let range = prefetch_range(
            total,
            &self.state.navigation.content_visible,
            self.state.navigation.content_selected,
            self.state.navigation.content_columns,
        );
        let urls = content_cover_urls(content, range);
        let covers = Arc::clone(&self.content_covers);
        let requests = Arc::clone(&self.cover_requests);
        let retry_after = Arc::clone(&self.cover_retry_after);
        let now = std::time::Instant::now();
        // Drop expired backoff entries; otherwise every failed cover URL stays
        // in this map for the whole session.
        lock_recover(&retry_after).retain(|_, until| *until > now);
        let missing: Vec<String> = urls
            .into_iter()
            .filter(|url| {
                if lock_recover(&covers).contains_key(url)
                    || lock_recover(&retry_after)
                        .get(url)
                        .is_some_and(|until| *until > now)
                {
                    return false;
                }
                lock_recover(&requests).insert(url.clone())
            })
            .collect();
        if missing.is_empty() {
            return;
        }
        log::debug!("content covers: total={total}, fetching={}", missing.len());

        let http = self.cover_http.clone();
        let picker = self.picker.clone();
        let sender = self.events.sender();

        tokio::spawn(async move {
            // Fetch a few at a time: one slow/broken URL must not hold up every
            // cover after it, and a whole screenful of requests at once makes
            // image hosts rate-limit the tail.
            let semaphore = Arc::new(tokio::sync::Semaphore::new(4));
            let mut tasks = tokio::task::JoinSet::new();
            // The event loop has no tick: every Redraw repaints the whole UI, so
            // coalesce the per-cover signals into at most one per interval
            // instead of one per thumbnail (up to 60 at once).
            let pending_redraw = Arc::new(AtomicBool::new(false));
            for url in missing {
                let Ok(permit) = Arc::clone(&semaphore).acquire_owned().await else {
                    break;
                };
                let http = http.clone();
                let picker = picker.clone();
                let covers = Arc::clone(&covers);
                let requests = Arc::clone(&requests);
                let retry_after = Arc::clone(&retry_after);
                let pending_redraw = Arc::clone(&pending_redraw);
                tasks.spawn(async move {
                    let _permit = permit;
                    let fetched =
                        tokio::time::timeout(Duration::from_secs(15), fetch_cover(&http, &url, px))
                            .await
                            .ok()
                            .flatten();
                    let Some(bytes) = fetched else {
                        log::warn!("content cover failed: {url}");
                        lock_recover(&requests).remove(&url);
                        lock_recover(&retry_after)
                            .insert(url, std::time::Instant::now() + Duration::from_secs(30));
                        return;
                    };
                    let protocol = tokio::task::spawn_blocking(move || {
                        thumb_protocol(&bytes, picker, round, px)
                    })
                    .await
                    .ok()
                    .flatten();
                    match protocol {
                        Some(protocol) => {
                            // Success must also clear the in-flight marker: the
                            // covers map already prevents re-fetching, but
                            // leaving the URL behind grows `requests` forever.
                            lock_recover(&requests).remove(&url);
                            lock_recover(&retry_after).remove(&url);
                            lock_recover(&covers).insert(url, protocol);
                            pending_redraw.store(true, Ordering::SeqCst);
                        }
                        None => {
                            log::warn!("content cover decode failed: {url}");
                            lock_recover(&requests).remove(&url);
                            lock_recover(&retry_after)
                                .insert(url, std::time::Instant::now() + Duration::from_secs(30));
                        }
                    }
                });
            }
            let mut last_redraw: Option<std::time::Instant> = None;
            while tasks.join_next().await.is_some() {
                let due = last_redraw.is_none_or(|t| t.elapsed() >= REDRAW_MIN_INTERVAL);
                if pending_redraw.load(Ordering::SeqCst) && due {
                    pending_redraw.store(false, Ordering::SeqCst);
                    let _ = sender.send(AppEvent::Redraw);
                    last_redraw = Some(std::time::Instant::now());
                }
            }
            // Flush a signal left over from the last throttled window.
            if pending_redraw.swap(false, Ordering::SeqCst) {
                let _ = sender.send(AppEvent::Redraw);
            }
        });
    }

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

/// Cover URLs around the visible cards: one screen of margin on both sides.
/// Keeping this windowed means a long playlist only downloads the covers the
/// user is actually approaching.
fn prefetch_range(
    total: usize,
    visible: &std::ops::Range<usize>,
    selected: usize,
    columns: usize,
) -> std::ops::Range<usize> {
    // Short lists are cheap enough to fetch whole, and that removes the
    // dependency on the renderer's visible range entirely.
    if total <= PREFETCH_WHOLE_LIST_MAX {
        return 0..total;
    }

    // Always include a window around the cursor: the renderer's visible range
    // can lag a frame behind while scrolling, and the cursor never does.
    let around = columns.max(1) * PREFETCH_ROWS;
    if visible.is_empty() {
        return selected.saturating_sub(around)
            ..selected.saturating_add(around).saturating_add(1).min(total);
    }

    let margin = visible.len().max(PREFETCH_MARGIN_MIN);
    let start = visible
        .start
        .saturating_sub(margin)
        .min(selected.saturating_sub(around));
    let end = visible
        .end
        .saturating_add(margin)
        .max(selected.saturating_add(around).saturating_add(1))
        .min(total);
    start..end
}

/// Cover URLs in the prefetch range, de-duplicated while preserving order.
fn content_cover_urls<'a>(content: &'a ContentState, range: std::ops::Range<usize>) -> Vec<String> {
    let count = range.end.saturating_sub(range.start);
    // Borrow URLs for de-duplication and allocate only the returned strings.
    let mut seen: HashSet<&'a str> = HashSet::with_capacity(count);
    let mut urls = Vec::with_capacity(count);
    let mut push = |url: &'a str| {
        if !url.is_empty() && seen.insert(url) {
            urls.push(url.to_string());
        }
    };
    match content {
        ContentState::Songs(songs) => {
            for song in songs.iter().skip(range.start).take(count) {
                push(&song.pic_url);
            }
        }
        ContentState::SongLists(lists) => {
            for list in lists.iter().skip(range.start).take(count) {
                push(&list.cover_img_url);
            }
        }
        ContentState::TopLists(lists) => {
            for list in lists.iter().skip(range.start).take(count) {
                push(&list.cover);
            }
        }
        ContentState::Singers(singers) => {
            for singer in singers.iter().skip(range.start).take(count) {
                push(&singer.pic_url);
            }
        }
        _ => {}
    }
    urls
}

async fn fetch_cover(http: &reqwest::Client, url: &str, px: u32) -> Option<Vec<u8>> {
    let url = sized_cover_url(url, px);
    let response = http.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.bytes().await.ok().map(|bytes| bytes.to_vec())
}

fn thumb_protocol(
    data: &[u8],
    picker: ratatui_image::picker::Picker,
    round: bool,
    px: u32,
) -> Option<StatefulProtocol> {
    let image = image::load_from_memory(data).ok()?;
    // Keep the whole image so the card shows its subject; the card sizes the
    // cover by the real terminal font ratio and uses `Resize::Fit` to scale.
    let small = image.thumbnail(px, px);
    Some(picker.new_resize_protocol(if round { round_thumbnail(small) } else { small }))
}

/// Crop to a square and fade everything outside the inscribed circle to
/// transparent, with a soft edge. Used for singer portraits.
fn round_thumbnail(img: image::DynamicImage) -> image::DynamicImage {
    let (width, height) = img.dimensions();
    let size = width.min(height);
    let x = (width - size) / 2;
    let y = (height - size) / 2;
    let mut square = img.crop_imm(x, y, size, size).to_rgba8();

    let radius = size as f32 / 2.0;
    for (px, py, pixel) in square.enumerate_pixels_mut() {
        let dx = px as f32 + 0.5 - radius;
        let dy = py as f32 + 0.5 - radius;
        let coverage = (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
        pixel.0[3] = (pixel.0[3] as f32 * coverage) as u8;
    }
    image::DynamicImage::ImageRgba8(square)
}

/// Longest side decoded for content-card thumbnails. A card's cover area is at
/// most 12 cells wide and a few rows tall, so a typical 8×16 cell font only
/// needs a ~48px square; 96px leaves 2× headroom for larger fonts and
/// high-DPI terminals. This also caps sources that ignore the CDN's resize
/// parameters.
pub(crate) const CONTENT_COVER_MAX_PX: u32 = 96;

/// Singer portraits are shown as big circles, so they are fetched and decoded
/// larger than song covers. 128px (≈65 KiB RGBA) keeps the cache cheap while
/// still covering a 16-cell-wide portrait on common fonts.
pub(crate) const SINGER_COVER_MAX_PX: u32 = 128;

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
    crate::playback::memory::trim_heap();
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
    use super::{
        COVER_MAX_PX, PREFETCH_ROWS, downscale_cover, prefetch_range, round_thumbnail,
        sized_cover_url,
    };

    #[test]
    fn round_thumbnail_masks_the_corners() {
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            20,
            10,
            image::Rgba([200, 100, 50, 255]),
        ));
        let round = round_thumbnail(img);
        assert_eq!((round.width(), round.height()), (10, 10));
        let rgba = round.to_rgba8();
        assert_eq!(rgba.get_pixel(0, 0)[3], 0, "corner must be transparent");
        assert_eq!(rgba.get_pixel(5, 5)[3], 255, "centre must stay opaque");
    }

    #[test]
    fn prefetch_range_fetches_short_lists_whole() {
        assert_eq!(prefetch_range(31, &(0..6), 0, 2), 0..31);
        assert_eq!(prefetch_range(60, &(0..6), 0, 2), 0..60);
    }

    #[test]
    fn prefetch_range_adds_a_margin_around_the_viewport() {
        assert_eq!(prefetch_range(1000, &(100..110), 100, 2), 90..120);
    }

    #[test]
    fn prefetch_range_always_covers_the_cursor() {
        let around = 2 * PREFETCH_ROWS;
        let range = prefetch_range(1000, &(0..0), 50, 2);
        assert_eq!(range, 50 - around..50 + around + 1);
        assert!(range.contains(&50));
    }

    #[test]
    fn prefetch_range_clamps_to_the_list() {
        assert_eq!(prefetch_range(200, &(185..200), 199, 2), 170..200);
    }

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
