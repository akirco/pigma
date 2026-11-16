//! TOML configuration: the runtime `Config` plus the border/cache/column/
//! navigation/playerbar/theme registries.

mod border;
mod cache;
mod column;
mod content;
mod equalizer;
mod fallback;
mod file;
mod format;
mod gradient;
mod logger;
mod navigation;
mod playerbar;
mod proxy;
mod theme;
mod titles;

pub use border::*;
pub use cache::*;
pub use column::*;
pub use content::{ContentConfig, ContentStyle};
pub use equalizer::*;
pub use fallback::*;
pub use file::*;
pub use gradient::*;
pub use logger::Logger;
pub use navigation::*;
pub use playerbar::*;
pub use proxy::*;
pub use theme::{Theme, ThemeRegistry, theme_fallback};
pub use titles::*;
