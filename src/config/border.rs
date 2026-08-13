use serde::{Deserialize, Serialize};

use crate::utils::GradientPreset;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorderConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub rounded: bool,
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
            rounded: false,
            follow_corner_color: false,
            border_gradient: None,
            border_gradient_speed: 0.0,
        }
    }
}
