use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorderConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub rounded: bool,
    /// 横竖边框是否跟随 corner_color 的颜色
    #[serde(default)]
    pub follow_corner_color: bool,
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
        }
    }
}
