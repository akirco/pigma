use crate::config::{Config, Theme, ThemeRegistry, theme_fallback};

use super::App;

impl App {
    /// 解析当前主题：优先 `default_theme`，缺失则回退 `default`，再缺失用硬编码兜底。
    /// 取字段借用而非整个 `&self`，便于调用方在不持有整体借用的前提下使用。
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
