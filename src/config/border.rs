use serde::{Deserialize, Deserializer, Serialize};

use crate::config::{GradientPreset, deserialize_optional};

/// Character set used to draw a border.
///
/// The named variants match `ratatui::widgets::BorderType` and are serialized
/// in `snake_case`, e.g. `style = "light_double_dashed"`. `Custom` takes its
/// characters from [`BorderConfig::chars`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BorderStyle {
    #[default]
    Plain,
    Rounded,
    Double,
    Thick,
    LightDoubleDashed,
    HeavyDoubleDashed,
    LightTripleDashed,
    HeavyTripleDashed,
    LightQuadrupleDashed,
    HeavyQuadrupleDashed,
    QuadrantInside,
    QuadrantOutside,
    /// Characters come from [`BorderChars`]; missing slots keep the plain glyph.
    Custom,
}

impl BorderStyle {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "plain" => Some(Self::Plain),
            "rounded" => Some(Self::Rounded),
            "double" => Some(Self::Double),
            "thick" => Some(Self::Thick),
            "light_double_dashed" => Some(Self::LightDoubleDashed),
            "heavy_double_dashed" => Some(Self::HeavyDoubleDashed),
            "light_triple_dashed" => Some(Self::LightTripleDashed),
            "heavy_triple_dashed" => Some(Self::HeavyTripleDashed),
            "light_quadruple_dashed" => Some(Self::LightQuadrupleDashed),
            "heavy_quadruple_dashed" => Some(Self::HeavyQuadrupleDashed),
            "quadrant_inside" => Some(Self::QuadrantInside),
            "quadrant_outside" => Some(Self::QuadrantOutside),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

/// Unknown styles fall back to `plain` instead of failing the whole config.
impl<'de> Deserialize<'de> for BorderStyle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from_name(&raw).unwrap_or_default())
    }
}

/// One-cell characters used by [`BorderStyle::Custom`]. Missing fields fall
/// back to the plain border glyph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BorderChars {
    pub top_left: String,
    pub top_right: String,
    pub bottom_left: String,
    pub bottom_right: String,
    pub horizontal_top: String,
    pub horizontal_bottom: String,
    pub vertical_left: String,
    pub vertical_right: String,
}

impl Default for BorderChars {
    fn default() -> Self {
        Self {
            top_left: "┌".to_string(),
            top_right: "┐".to_string(),
            bottom_left: "└".to_string(),
            bottom_right: "┘".to_string(),
            horizontal_top: "─".to_string(),
            horizontal_bottom: "─".to_string(),
            vertical_left: "│".to_string(),
            vertical_right: "│".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorderConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Border character set. Defaults to `plain`; `b` only toggles visibility,
    /// so the style itself is chosen here in the config.
    #[serde(default)]
    pub style: BorderStyle,
    /// Characters for `style = "custom"`; missing slots fall back to plain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chars: Option<BorderChars>,
    /// Whether the horizontal and vertical borders follow corner_color
    #[serde(default)]
    pub follow_corner_color: bool,
    /// Border gradient preset (warm / rainbow / turbo / spectral / viridis / cubehelix)
    /// None = solid color, Some = gradient
    #[serde(default, deserialize_with = "deserialize_optional")]
    pub border_gradient: Option<GradientPreset>,
    /// Gradient animation speed, 0 = static, >0 = animated
    #[serde(default)]
    pub border_gradient_speed: f64,
}

fn default_true() -> bool {
    true
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            style: BorderStyle::default(),
            chars: None,
            follow_corner_color: false,
            border_gradient: None,
            border_gradient_speed: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names_are_snake_case() {
        let config: BorderConfig =
            serde_json::from_str(r#"{"enabled":true,"style":"light_double_dashed"}"#)
                .expect("parse border config");
        assert_eq!(config.style, BorderStyle::LightDoubleDashed);
    }

    #[test]
    fn custom_style_defaults_missing_slots_to_plain() {
        let config: BorderConfig = serde_json::from_str(
            r#"{
                "enabled": true,
                "style": "custom",
                "chars": { "top_left": "◆", "horizontal_top": "╌" }
            }"#,
        )
        .expect("parse custom border");
        assert_eq!(config.style, BorderStyle::Custom);
        let chars = config.chars.expect("custom chars");
        assert_eq!(chars.top_left, "◆");
        assert_eq!(chars.horizontal_top, "╌");
        assert_eq!(chars.top_right, "┐", "missing slot keeps the plain glyph");
    }

    #[test]
    fn style_defaults_to_plain() {
        let config: BorderConfig =
            serde_json::from_str(r#"{"enabled":true}"#).expect("parse border config without style");
        assert_eq!(config.style, BorderStyle::Plain);
    }
}
