use serde::{Deserialize, Deserializer, Serialize};

use super::{GradientPreset, deserialize_optional};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LayoutType {
    Classic,
    #[default]
    Modern,
}

impl LayoutType {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            // `default` is the pre-rename spelling of `classic`.
            "classic" | "default" => Some(Self::Classic),
            "modern" => Some(Self::Modern),
            _ => None,
        }
    }
}

/// Unknown layouts fall back to the default layout instead of failing the
/// whole config.
impl<'de> Deserialize<'de> for LayoutType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from_name(&raw).unwrap_or_default())
    }
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
    /// Progress bar gradient preset. An empty string or unknown name disables the gradient.
    #[serde(default, deserialize_with = "deserialize_optional")]
    pub gradient_preset: Option<GradientPreset>,
    #[serde(default)]
    pub layout: LayoutType,
    #[serde(default)]
    pub visible: PlayerbarVisible,
}

fn default_pb_filled_symbol() -> String {
    crate::icons::PROGRESS_FILLED.into()
}
fn default_pb_unfilled_symbol() -> String {
    crate::icons::PROGRESS_UNFILLED.into()
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

#[cfg(test)]
mod tests {
    use super::LayoutType;

    #[test]
    fn modern_is_the_default_layout() {
        assert_eq!(LayoutType::default(), LayoutType::Modern);
    }

    #[test]
    fn layout_names_are_classic_and_modern() {
        assert_eq!(LayoutType::from_name("classic"), Some(LayoutType::Classic));
        assert_eq!(LayoutType::from_name("default"), Some(LayoutType::Classic));
        assert_eq!(LayoutType::from_name("modern"), Some(LayoutType::Modern));
        assert_eq!(LayoutType::from_name("minimal"), None);
    }
}
