use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

/// 歌词高亮渐变预设
///
/// 复刻 colorgrad 预设的真实算法
///
/// - warm / cubehelix：Cubehelix 色彩模型（hue 插值）
/// - rainbow：Cubehelix 逐点公式
/// - turbo：5 次多项式（colorgrad 原实现）
/// - spectral / viridis：精确 hex 色站 + RGB 线性插值（BlendMode::Rgb）
///   未知名称回退到 rainbow
///   Gradient preset enum — avoids per-call string comparison in hot render paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradientPreset {
    #[default]
    Rainbow,
    Warm,
    Cubehelix,
    Turbo,
    Spectral,
    Viridis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GradientPresetError(String);

impl fmt::Display for GradientPresetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown gradient preset: {}", self.0)
    }
}

impl std::error::Error for GradientPresetError {}

impl FromStr for GradientPreset {
    type Err = GradientPresetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("warm") {
            Ok(Self::Warm)
        } else if s.eq_ignore_ascii_case("cubehelix") {
            Ok(Self::Cubehelix)
        } else if s.eq_ignore_ascii_case("rainbow") {
            Ok(Self::Rainbow)
        } else if s.eq_ignore_ascii_case("turbo") {
            Ok(Self::Turbo)
        } else if s.eq_ignore_ascii_case("spectral") {
            Ok(Self::Spectral)
        } else if s.eq_ignore_ascii_case("viridis") {
            Ok(Self::Viridis)
        } else {
            Err(GradientPresetError(s.to_owned()))
        }
    }
}

impl GradientPreset {
    /// Parse with fallback to `Rainbow` for unrecognized names.
    pub fn from_str_or_rainbow(s: &str) -> Self {
        s.parse().unwrap_or_default()
    }

    pub fn color(self, t: f32) -> [u8; 3] {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Cubehelix => cubehelix_color(-100.0, 0.5, 0.0, -240.0, 0.5, 1.0, t),
            Self::Rainbow => {
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
            Self::Turbo => turbo_color(t),
            Self::Spectral => interp_stops(
                &[
                    0x9e0142, 0xd53e4f, 0xf46d43, 0xfdae61, 0xfee08b, 0xffffbf, 0xe6f598, 0xabdda4,
                    0x66c2a5, 0x3288bd, 0x5e4fa2,
                ],
                t,
            ),
            Self::Viridis => interp_stops(
                &[
                    0x440154, 0x482777, 0x3f4a8a, 0x31678e, 0x26838f, 0x1f9d8a, 0x6cce5a, 0xb6de2b,
                    0xfee825,
                ],
                t,
            ),
            Self::Warm => cubehelix_color(-100.0, 0.75, 0.35, 80.0, 1.5, 0.8, t),
        }
    }
}

// Keep the string-based function for callers that receive runtime strings
// (e.g. styled_text markup parser). Hot-path callers should use
// GradientPreset::from_str_or_rainbow + .color() directly.
pub fn gradient_color(preset: &str, t: f32) -> [u8; 3] {
    GradientPreset::from_str_or_rainbow(preset).color(t)
}

/// Deserialize an optional preset: empty or unknown names yield `None`
/// (gradient disabled) instead of failing the whole config parse.
pub fn deserialize_optional<'de, D>(deserializer: D) -> Result<Option<GradientPreset>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(GradientPreset::from_str(&s).ok())
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
    fn gradient_warm_bounds() {
        assert_eq!(GradientPreset::Warm.color(0.0), [110, 64, 170]);
        let end = GradientPreset::Warm.color(1.0);
        assert_eq!(end, [175, 240, 91]);
    }

    #[test]
    fn gradient_unknown_fallback() {
        assert_eq!(
            GradientPreset::from_str_or_rainbow("nope").color(1.0),
            GradientPreset::Rainbow.color(1.0)
        );
    }

    #[test]
    fn gradient_rainbow_matches_colorgrad() {
        assert_eq!(GradientPreset::Rainbow.color(0.25), [255, 94, 99]);
        assert_eq!(GradientPreset::Rainbow.color(0.75), [26, 199, 194]);
    }

    #[test]
    fn gradient_turbo_bounds() {
        assert_eq!(GradientPreset::Turbo.color(0.0), [35, 23, 27]);
        assert_eq!(GradientPreset::Turbo.color(1.0), [144, 12, 0]);
    }

    #[test]
    fn gradient_viridis_bounds() {
        assert_eq!(GradientPreset::Viridis.color(0.0), [68, 1, 84]);
        assert_eq!(GradientPreset::Viridis.color(1.0), [182, 222, 43]);
    }

    #[test]
    fn from_str_case_insensitive() {
        assert_eq!(
            "Warm".parse::<GradientPreset>().unwrap(),
            GradientPreset::Warm
        );
        assert_eq!(
            "TURBO".parse::<GradientPreset>().unwrap(),
            GradientPreset::Turbo
        );
        assert_eq!(
            "Spectral".parse::<GradientPreset>().unwrap(),
            GradientPreset::Spectral
        );
    }

    #[test]
    fn string_fn_matches_enum() {
        for name in &[
            "warm",
            "cubehelix",
            "rainbow",
            "turbo",
            "spectral",
            "viridis",
            "nope",
        ] {
            for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                assert_eq!(
                    gradient_color(name, t),
                    GradientPreset::from_str_or_rainbow(name).color(t)
                );
            }
        }
    }
}
