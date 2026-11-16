use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NavConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<NavSectionConfig>,
    /// Characters drawn on each side of the **selected or hovered** item.
    #[serde(default = "default_selected_left")]
    pub selected_left: String,
    #[serde(default = "default_selected_right")]
    pub selected_right: String,
}

fn default_selected_left() -> String {
    crate::icons::NAV_CAPSULE_START.to_string()
}

fn default_selected_right() -> String {
    crate::icons::NAV_CAPSULE_END.to_string()
}

impl NavConfig {
    /// The display name of the nav item whose `api` matches `api_str`, if any.
    /// Lets headless/daemon queue keys match what the TUI would show for the
    /// same endpoint (e.g. `liked` → " 我喜欢的音乐").
    pub fn name_for_api(&self, api_str: &str) -> Option<String> {
        self.sections
            .iter()
            .flat_map(|s| &s.items)
            .find(|item| item.api.as_deref() == Some(api_str))
            .map(|item| item.name.clone())
    }
}

impl Default for NavConfig {
    fn default() -> Self {
        Self {
            selected_left: default_selected_left(),
            selected_right: default_selected_right(),
            sections: vec![
                NavSectionConfig {
                    title: format!(
                        "<accent>{}</accent> <accent>DISCOVER</accent>",
                        crate::icons::SECTION_MARK
                    ),
                    items: vec![
                        NavItemConfig {
                            name: " 每日推荐".into(),
                            api: Some("recommend_songs".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 推荐歌单".into(),
                            api: Some("recommend_resource".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 热门歌手".into(),
                            api: Some("top_singers".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 排行榜".into(),
                            api: Some("toplist".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 歌单".into(),
                            api: Some("top_song_list".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 电台".into(),
                            api: Some("user_radio_sublist".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 搜索".into(),
                            api: Some("search".into()),
                            title_template: None,
                        },
                    ],
                },
                NavSectionConfig {
                    title: format!(
                        "<accent>{}</accent> <accent>MY MUSIC</accent>",
                        crate::icons::SECTION_MARK
                    ),
                    items: vec![
                        NavItemConfig {
                            name: " 我喜欢的音乐".into(),
                            api: Some("liked".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 我创建的歌单".into(),
                            api: Some("user_created_song_list".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 我收藏的歌单".into(),
                            api: Some("user_subscribed_song_list".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 我收藏的专辑".into(),
                            api: Some("album_sublist".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 我的音乐云盘".into(),
                            api: Some("user_cloud_disk".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 下载管理".into(),
                            api: Some("download".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 本地音乐".into(),
                            api: Some("local_music".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: " 最近播放".into(),
                            api: Some("recent".into()),
                            title_template: None,
                        },
                    ],
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavSectionConfig {
    pub title: String,
    pub items: Vec<NavItemConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavItemConfig {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    /// Optional title template. Supports `{name}` (item name), `{count}` (item count).
    /// If None, defaults to `"a"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_template: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NavPosition {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

impl NavPosition {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }
}

/// Unknown positions fall back to `left` instead of failing the whole config.
impl<'de> Deserialize<'de> for NavPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from_name(&raw).unwrap_or_default())
    }
}

impl NavPosition {
    /// The next position in the left → right → top → bottom cycle.
    pub fn cycle(self) -> Self {
        match self {
            NavPosition::Left => NavPosition::Right,
            NavPosition::Right => NavPosition::Top,
            NavPosition::Top => NavPosition::Bottom,
            NavPosition::Bottom => NavPosition::Left,
        }
    }

    /// Human-readable Chinese label used for toasts.
    pub fn label(self) -> &'static str {
        match self {
            NavPosition::Left => "左侧",
            NavPosition::Right => "右侧",
            NavPosition::Top => "顶部",
            NavPosition::Bottom => "底部",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NavConfig;

    #[test]
    fn default_selected_pads_use_the_powerline_separators() {
        let config = NavConfig::default();
        assert_eq!(config.selected_left, "\u{e0b2}");
        assert_eq!(config.selected_right, "\u{e0b0}");
    }
}
