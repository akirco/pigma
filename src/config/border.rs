use serde::{Deserialize, Serialize};

use crate::config::GradientPreset;

/// Character set used to draw a border.
///
/// The names match `ratatui::widgets::BorderType` and are serialized in
/// `snake_case`, e.g. `style = "light_double_dashed"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorderConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Border character set. Defaults to `plain`; `b` only toggles visibility,
    /// so the style itself is chosen here in the config.
    #[serde(default)]
    pub style: BorderStyle,
    /// Whether the horizontal and vertical borders follow corner_color
    #[serde(default)]
    pub follow_corner_color: bool,
    /// Border gradient preset (warm / rainbow / turbo / spectral / viridis / cubehelix)
    /// None = solid color, Some = gradient
    #[serde(default)]
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
    fn style_defaults_to_plain() {
        let config: BorderConfig =
            serde_json::from_str(r#"{"enabled":true}"#).expect("parse border config without style");
        assert_eq!(config.style, BorderStyle::Plain);
    }
}
