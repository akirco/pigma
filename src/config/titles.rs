use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitlesConfig {
    #[serde(default = "default_title_sidebar")]
    pub sidebar: String,
    #[serde(default = "default_title_playlist")]
    pub playlist: String,
    #[serde(default = "default_title_lyrics")]
    pub lyrics: String,
}

fn default_title_sidebar() -> String {
    format!(
        "{} NAVIGATION {}",
        crate::icons::TITLE_START,
        crate::icons::TITLE_END
    )
}
fn default_title_playlist() -> String {
    format!(
        "{} {} QUEUE ({{count}}) {}",
        crate::icons::TITLE_START,
        crate::icons::MUSIC_NOTE,
        crate::icons::TITLE_END
    )
}
fn default_title_lyrics() -> String {
    format!(
        "{} {} LYRICS {}",
        crate::icons::TITLE_START,
        crate::icons::MUSIC_NOTE,
        crate::icons::TITLE_END
    )
}

impl Default for TitlesConfig {
    fn default() -> Self {
        Self {
            sidebar: default_title_sidebar(),
            playlist: default_title_playlist(),
            lyrics: default_title_lyrics(),
        }
    }
}
