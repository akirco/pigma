use crate::config::{Config, Theme, ThemeRegistry, theme_fallback};

use super::App;

impl App {
    /// Resolve the current theme: prefer `default_theme`, fall back to `default`
    /// if missing, then to a hardcoded fallback.
    /// Borrows individual fields rather than the whole `&self` so callers can use
    /// it without holding an overall borrow.
    pub(crate) fn resolve_theme<'a>(config: &Config, registry: &'a ThemeRegistry) -> &'a Theme {
        registry.get(&config.default_theme).unwrap_or_else(|| {
            log::warn!(
                "Theme '{}' not found, falling back to default",
                config.default_theme
            );
            registry.get("default").unwrap_or_else(|| {
                log::error!("Default theme missing, using hardcoded fallback");
                theme_fallback()
            })
        })
    }

    pub fn current_theme(&self) -> &Theme {
        Self::resolve_theme(&self.config, &self.theme_registry)
    }
}
