//! Render-ready cover assets: the bounded thumbnail cache behind the content
//! cards, plus the decode, mask and re-encode helpers that turn cover bytes
//! into terminal protocols.
//!
//! Fetching and the on-disk cache live in [`crate::cache::cover`]; scheduling —
//! when to prefetch, how to coalesce redraws, how to drop a stale load — stays
//! in `app/cover.rs`. This module owns the presentation-side types so the
//! cache layer never has to name `ratatui_image`.

mod cache;

use std::sync::{Arc, Mutex};

use image::GenericImageView;
use ratatui_image::protocol::StatefulProtocol;

/// Cover thumbnails for the content cards, keyed by the original cover URL.
pub(crate) type ContentCovers = Arc<Mutex<cache::CoverCache>>;

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
pub(crate) const COVER_MAX_PX: u32 = 200;

/// A decoded cover plus its small JPEG counterpart for the disk cache.
pub(crate) struct PreparedCover {
    pub(crate) protocol: StatefulProtocol,
    /// `None` when re-encoding failed; the cover still renders, it just is not
    /// written back to the cache.
    pub(crate) thumbnail: Option<Vec<u8>>,
}

pub(crate) fn thumb_protocol(
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

/// Decode cover bytes, downsample, apply the circular mask, build the resize
/// protocol, and re-encode a small JPEG for the cache.
pub(crate) fn build_cover_protocol(
    data: &[u8],
    picker: ratatui_image::picker::Picker,
) -> Option<PreparedCover> {
    let prepared = build_cover_protocol_inner(data, picker);
    // The inner call has dropped the decoded buffers by now. glibc keeps freed
    // pages on its free lists, so return them to the OS instead of leaving RSS
    // at the decode peak for the rest of the session.
    crate::memory::trim_heap();
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

/// Fit a cover inside [`COVER_MAX_PX`] without upscaling small images.
fn downscale_cover(img: image::DynamicImage) -> image::DynamicImage {
    if img.width() > COVER_MAX_PX || img.height() > COVER_MAX_PX {
        img.thumbnail(COVER_MAX_PX, COVER_MAX_PX)
    } else {
        img
    }
}

#[cfg(test)]
mod tests {
    use super::{COVER_MAX_PX, downscale_cover, round_thumbnail};

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
}
