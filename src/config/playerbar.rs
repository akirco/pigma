use serde::{Deserialize, Serialize};

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
    "warning".into()
}

impl Default for PlayerbarConfig {
    fn default() -> Self {
        Self {
            filled_symbol: default_pb_filled_symbol(),
            unfilled_symbol: default_pb_unfilled_symbol(),
            filled_color: default_pb_filled_color(),
            unfilled_color: default_pb_unfilled_color(),
            unfilled_color_cached: default_pb_unfilled_color_cached(),
        }
    }
}
