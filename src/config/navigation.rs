use serde::{Deserialize, Serialize};

use crate::api::ApiEndpoint;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<NavSectionConfig>,
}

impl Default for NavConfig {
    fn default() -> Self {
        Self {
            sections: vec![
                NavSectionConfig {
                    title: "<accent>▎</accent> <b>DISCOVER</b>".into(),
                    items: vec![
                        NavItemConfig {
                            name: "每日推荐".into(),
                            api: Some("recommend_songs".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "推荐歌单".into(),
                            api: Some("recommend_resource".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "排行榜".into(),
                            api: Some("toplist".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "歌单".into(),
                            api: Some("top_song_list".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "电台".into(),
                            api: Some("user_radio_sublist".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "搜索".into(),
                            api: Some("search".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "热门歌手".into(),
                            api: Some("top_singers".into()),
                            title_template: None,
                        },
                    ],
                },
                NavSectionConfig {
                    title: "<accent>▎</accent> <b>MY MUSIC</b>".into(),
                    items: vec![
                        NavItemConfig {
                            name: "我的音乐云盘".into(),
                            api: Some("user_cloud_disk".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "我喜欢的音乐".into(),
                            api: Some("__liked__".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "我的歌单".into(),
                            api: Some("user_song_list".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "下载管理".into(),
                            api: Some("__download__".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "本地音乐".into(),
                            api: Some("__local_music__".into()),
                            title_template: None,
                        },
                        NavItemConfig {
                            name: "最近播放".into(),
                            api: Some("__recent__".into()),
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
    /// If None, defaults to `"{name} ({count})"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_template: Option<String>,
}

impl NavItemConfig {
    pub fn endpoint(&self) -> Option<ApiEndpoint> {
        self.api.as_deref().and_then(ApiEndpoint::parse)
    }
}
