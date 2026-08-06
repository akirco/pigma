use serde::{Deserialize, Serialize};

use crate::utils::{GradientPreset, deserialize_optional};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LayoutType {
    #[default]
    Default,
    Modern,
    Minimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerbarVisible {
    pub cover: bool,
    pub volume: bool,
    pub mode_icon: bool,
    pub spinner: bool,
}

impl Default for PlayerbarVisible {
    fn default() -> Self {
        Self {
            cover: true,
            volume: true,
            mode_icon: true,
            spinner: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerbarConfig {
    #[serde(default = "default_pb_filled_symbol")]
    pub filled_symbol: String,
    #[serde(default = "default_pb_unfilled_symbol")]
    pub unfilled_symbol: String,
    #[serde(default = "default_pb_filled_color")]
    pub filled_color: String,
    #[serde(default = "default_pb_unfilled_color")]
    pub unfilled_color: String,
    #[serde(default = "default_pb_unfilled_color_cached")]
    pub unfilled_color_cached: String,
    /// 进度条渐变预设。空串或未知名称表示不启用渐变。
    #[serde(default, deserialize_with = "deserialize_optional")]
    pub gradient_preset: Option<GradientPreset>,
    #[serde(default)]
    pub layout: LayoutType,
    #[serde(default)]
    pub visible: PlayerbarVisible,
}

fn default_pb_filled_symbol() -> String {
    "━".into()
}
fn default_pb_unfilled_symbol() -> String {
    "─".into()
}
fn default_pb_filled_color() -> String {
    "accent".into()
}
fn default_pb_unfilled_color() -> String {
    "text".into()
}
fn default_pb_unfilled_color_cached() -> String {
    "error".into()
}

impl Default for PlayerbarConfig {
    fn default() -> Self {
        Self {
            filled_symbol: default_pb_filled_symbol(),
            unfilled_symbol: default_pb_unfilled_symbol(),
            filled_color: default_pb_filled_color(),
            unfilled_color: default_pb_unfilled_color(),
            unfilled_color_cached: default_pb_unfilled_color_cached(),
            gradient_preset: None,
            layout: LayoutType::default(),
            visible: PlayerbarVisible::default(),
        }
    }
}
