//! Theme and gradient primitives shared by configuration and TUI widgets.

mod gradient;
mod palette;

pub use gradient::{GradientPreset, deserialize_optional, gradient_color};
pub use palette::{Theme, ThemeRegistry, theme_fallback};
