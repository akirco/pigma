use std::sync::Arc;

use ratatui_image::picker::Picker;
use reqwest::Client;
use sonar::SonarFinder;

use crate::config::{Config, ProxyTarget, ThemeRegistry};
use crate::state::{CommandAction, CommandItem, CommandPanel};
use crate::utils::terminal::{ImageProtocol, best_image_protocol};

use super::App;

/// 用于按 `ProxyTarget` 计算是否启用代理。
pub(super) enum ProxyKind {
    /// 非 YouTube 服务（网易云、sonar 搜索、封面、流媒体），在 `Reversed`/`Both` 下走代理。
    NonYoutube,
    /// YouTube 服务，在 `Normal`/`Both` 下走代理。
    Youtube,
}

impl App {
    /// `NonYoutube` 在 `Reversed`/`Both` 下走代理，`Youtube` 在 `Normal`/`Both` 下走代理。
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

    /// 构建命令面板（主题切换子菜单 + 边框/边听边存开关）。
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
        ];

        let mut command_panel = CommandPanel::new();
        command_panel.levels = vec![commands];
        command_panel
    }

    /// 按配置构建 sonar finder，应用搜索/YouTube 代理。
    pub(super) fn build_finder(
        config: &Config,
        search_proxy: &str,
        youtube_proxy: &str,
    ) -> Arc<SonarFinder> {
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
        Arc::new(sonar::SonarFinder::new(search_config))
    }

    /// 按当前终端构建图片 picker，并选择最佳的图片协议（Kitty/Sixel）。
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

    /// 按给定代理地址构建阻塞 HTTP client；地址为空则直连。
    pub(super) fn build_http_client(proxy: &str) -> color_eyre::Result<Client> {
        let mut builder = Client::builder();
        if !proxy.is_empty() {
            builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(color_eyre::Report::msg)?);
        }
        builder.build().map_err(color_eyre::Report::msg)
    }
}
