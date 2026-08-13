use std::sync::Arc;

use ratatui_image::picker::Picker;
use reqwest::Client;
use sonar::SonarFinder;

use crate::config::{Config, ProxyTarget, ThemeRegistry};
use crate::state::{CommandAction, CommandItem, CommandPanel};
use crate::utils::terminal::{ImageProtocol, best_image_protocol};

use super::App;

/// Used to decide whether a proxy is enabled based on `ProxyTarget`.
pub(super) enum ProxyKind {
    /// Non-YouTube services (NetEase Cloud, sonar search, covers, streaming), proxied under `Reversed`/`Both`.
    NonYoutube,
    /// YouTube services, proxied under `Normal`/`Both`.
    Youtube,
}

impl App {
    /// `NonYoutube` is proxied under `Reversed`/`Both`; `Youtube` under `Normal`/`Both`.
    pub(super) fn proxy_for(config: &Config, kind: ProxyKind) -> &str {
        let proxy = config.proxy.as_str();
        if proxy.is_empty() {
            return "";
        }
        let active = match kind {
            ProxyKind::NonYoutube => {
                matches!(
                    config.proxy_target,
                    ProxyTarget::Reversed | ProxyTarget::Both
                )
            }
            ProxyKind::Youtube => {
                matches!(config.proxy_target, ProxyTarget::Normal | ProxyTarget::Both)
            }
        };
        if active { proxy } else { "" }
    }

    /// Build the command panel (theme-switching submenu + border/save-on-play toggles).
    pub(super) fn build_command_panel(theme_registry: &ThemeRegistry) -> CommandPanel {
        let theme_children: Vec<CommandItem> = theme_registry
            .all_names()
            .into_iter()
            .map(|name| {
                let name = name.to_string();
                let action = CommandAction::SwitchTheme(name.clone());
                CommandItem::Action { name, action }
            })
            .collect();

        let commands = vec![
            CommandItem::SubMenu {
                name: "Switch Theme".into(),
                children: theme_children,
            },
            CommandItem::Action {
                name: "Toggle Border Mode".into(),
                action: CommandAction::ToggleBordered,
            },
            CommandItem::Action {
                name: "Toggle Save on Play".into(),
                action: CommandAction::ToggleSaveOnPlay,
            },
            CommandItem::Action {
                name: "Cycle Nav Position".into(),
                action: CommandAction::CycleNavPosition,
            },
        ];

        let mut command_panel = CommandPanel::new();
        command_panel.levels = vec![commands];
        command_panel
    }

    /// Build the sonar finder per config, applying the search/YouTube proxy.
    pub(super) fn build_finder(
        config: &Config,
        search_proxy: &str,
        youtube_proxy: &str,
    ) -> color_eyre::Result<Arc<SonarFinder>> {
        let mut sources: Vec<sonar::SonarSource> = Vec::new();
        for name in &config.source_fallback.providers {
            let source = match name.as_str() {
                "kuwo" => sonar::SonarSource::Kuwo,
                "kugou" => sonar::SonarSource::Kugou,
                "bilivideo" => sonar::SonarSource::BiliVideo,
                "youtube" => sonar::SonarSource::Youtube,
                _ => continue,
            };
            if !sources.contains(&source) {
                sources.push(source);
            }
        }
        let search_config = sonar::SearchConfig::new()
            .with_providers(sources)
            .with_timeout(config.source_fallback.timeout_ms)
            .with_search_proxy(search_proxy.to_string())
            .with_youtube_proxy(youtube_proxy.to_string());
        let finder = sonar::SonarFinder::new(search_config).map_err(color_eyre::Report::msg)?;
        Ok(Arc::new(finder))
    }

    /// Build the image picker for the current terminal and select the best image protocol (Kitty/Sixel).
    pub(super) fn build_picker() -> Picker {
        let mut picker = ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks());

        match best_image_protocol() {
            Some(ImageProtocol::Kitty) => {
                log::debug!("ImageProtocol::Kitty");
                picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
            }
            Some(ImageProtocol::Sixel) => {
                picker.set_protocol_type(ratatui_image::picker::ProtocolType::Sixel);
                log::debug!("ImageProtocol::Sixel");
            }
            None => {
                log::debug!("ImageProtocol::None");
            }
        }
        picker
    }

    /// Build a blocking HTTP client for the given proxy address; an empty address connects directly.
    pub(super) fn build_http_client(proxy: &str) -> color_eyre::Result<Client> {
        let mut builder = Client::builder();
        if !proxy.is_empty() {
            builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(color_eyre::Report::msg)?);
        }
        builder.build().map_err(color_eyre::Report::msg)
    }
}
