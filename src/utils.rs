use time::OffsetDateTime;
use time::format_description::FormatItem;
use time::macros::format_description;

/// `YYYY-MM-DD HH:MM:SS` in local timezone.
pub fn local_timestamp() -> String {
    let now = match time::OffsetDateTime::now_local() {
        Ok(t) => t,
        Err(_) => OffsetDateTime::now_utc(),
    };
    now.format(&TIMESTAMP_FMT)
        .unwrap_or_else(|_| String::from("0000-00-00 00:00:00"))
}

/// `HH:MM:SS` in local timezone (for UI clock display).
pub fn clock_time() -> String {
    local_timestamp()[11..].to_string()
}

const TIMESTAMP_FMT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

pub fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{:02}:{:02}", mins, secs)
}

/// Parse a `MM:SS` / `HH:MM:SS` duration string into total seconds.
pub fn parse_duration_secs(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split(':').collect();
    let mut secs: u64 = 0;
    for (i, p) in parts.iter().rev().enumerate() {
        let n: u64 = p.parse().ok()?;
        secs += n * 60u64.pow(i as u32);
    }
    Some(secs)
}

/// 歌词高亮渐变预设
///
/// 复刻 colorgrad 预设的真实算法
///
/// - warm / cubehelix：Cubehelix 色彩模型（hue 插值）
/// - rainbow：Cubehelix 逐点公式
/// - turbo：5 次多项式（colorgrad 原实现）
/// - spectral / viridis：精确 hex 色站 + RGB 线性插值（BlendMode::Rgb）
///
/// 未知名称回退到 warm。
pub fn gradient_color(preset: &str, t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    match preset.to_ascii_lowercase().as_str() {
        "cubehelix" => cubehelix_color(-100.0, 0.5, 0.0, -240.0, 0.5, 1.0, t),
        "rainbow" => {
            let ts = (t - 0.5).abs();
            cubehelix_color(
                360.0 * t - 100.0,
                1.5 - 1.5 * ts,
                0.8 - 0.9 * ts,
                360.0 * t - 100.0,
                1.5 - 1.5 * ts,
                0.8 - 0.9 * ts,
                t,
            )
        }
        "turbo" => turbo_color(t),
        "spectral" => interp_stops(
            &[
                0x9e0142, 0xd53e4f, 0xf46d43, 0xfdae61, 0xfee08b, 0xffffbf, 0xe6f598, 0xabdda4,
                0x66c2a5, 0x3288bd, 0x5e4fa2,
            ],
            t,
        ),
        "viridis" => interp_stops(
            &[
                0x440154, 0x482777, 0x3f4a8a, 0x31678e, 0x26838f, 0x1f9d8a, 0x6cce5a, 0xb6de2b,
                0xfee825,
            ],
            t,
        ),
        _ => cubehelix_color(-100.0, 0.75, 0.35, 80.0, 1.5, 0.8, t), // warm（默认）
    }
}

fn cubehelix_color(h0: f32, s0: f32, l0: f32, h1: f32, s1: f32, l1: f32, t: f32) -> [u8; 3] {
    let h = (h0 + t * (h1 - h0) + 120.0) * (std::f32::consts::PI / 180.0);
    let l = l0 + t * (l1 - l0);
    let s = s0 + t * (s1 - s0);
    let a = s * l * (1.0 - l);
    let cosh = h.cos();
    let sinh = h.sin();
    let r = l - a * (0.14861 * cosh - 1.78277 * sinh);
    let g = l - a * (0.29227 * cosh + 0.90649 * sinh);
    let b = l + a * (1.97294 * cosh);
    let cl = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [cl(r), cl(g), cl(b)]
}

fn turbo_color(t: f32) -> [u8; 3] {
    let r = 34.61 + t * (1172.33 - t * (10793.56 - t * (33300.12 - t * (38394.49 - t * 14825.05))));
    let g = 23.31 + t * (557.33 + t * (1225.33 - t * (3574.96 - t * (1073.77 + t * 707.56))));
    let b = 27.2 + t * (3211.1 - t * (15327.97 - t * (27814.0 - t * (22569.18 - t * 6838.66))));
    let cl = |v: f32| (v.clamp(0.0, 255.0).round() as u8).clamp(0, 255);
    [cl(r), cl(g), cl(b)]
}

fn interp_stops(stops: &[u32], t: f32) -> [u8; 3] {
    let n = stops.len();
    if n == 1 {
        return hex_rgb(stops[0]);
    }
    let x = t * (n - 1) as f32;
    let i = x.floor() as usize;
    let k = x - i as f32;
    let i = i.min(n - 2);
    let (r0, g0, b0) = {
        let [r, g, b] = hex_rgb(stops[i]);
        (r as f32, g as f32, b as f32)
    };
    let (r1, g1, b1) = {
        let [r, g, b] = hex_rgb(stops[i + 1]);
        (r as f32, g as f32, b as f32)
    };
    let mix = |a: f32, b: f32| (a + (b - a) * k).round() as u8;
    [mix(r0, r1), mix(g0, g1), mix(b0, b1)]
}

fn hex_rgb(c: u32) -> [u8; 3] {
    [
        ((c >> 16) & 0xff) as u8,
        ((c >> 8) & 0xff) as u8,
        (c & 0xff) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_hhmmss() {
        let c = clock_time();
        assert_eq!(c.len(), 8);
        assert_eq!(&c[2..3], ":");
        assert_eq!(&c[5..6], ":");
    }

    #[test]
    fn timestamp_len() {
        assert_eq!(local_timestamp().len(), 19);
    }

    #[test]
    fn parse_mmss() {
        assert_eq!(parse_duration_secs("3:30"), Some(210));
        assert_eq!(parse_duration_secs("1:03:30"), Some(3810));
    }

    #[test]
    fn gradient_warm_bounds() {
        // 对齐 colorgrad::preset::warm（Cubehelix 模型）
        assert_eq!(gradient_color("warm", 0.0), [110, 64, 170]);
        let end = gradient_color("warm", 1.0);
        assert_eq!(end, [175, 240, 91]);
    }

    #[test]
    fn gradient_unknown_fallback() {
        assert_eq!(gradient_color("nope", 1.0), gradient_color("warm", 1.0));
    }

    #[test]
    fn gradient_rainbow_matches_colorgrad() {
        // colorgrad 文档保证：at(0.25)=[255,94,99], at(0.75)=[26,199,194]
        assert_eq!(gradient_color("rainbow", 0.25), [255, 94, 99]);
        assert_eq!(gradient_color("rainbow", 0.75), [26, 199, 194]);
    }

    #[test]
    fn gradient_turbo_bounds() {
        assert_eq!(gradient_color("turbo", 0.0), [35, 23, 27]);
        assert_eq!(gradient_color("turbo", 1.0), [144, 12, 0]);
    }

    #[test]
    fn gradient_viridis_bounds() {
        assert_eq!(gradient_color("viridis", 0.0), [68, 1, 84]);
        assert_eq!(gradient_color("viridis", 1.0), [182, 222, 43]);
    }
}
