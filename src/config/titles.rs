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
    "\u{25BA} NAVIGATION \u{25C4}".into()
}
fn default_title_playlist() -> String {
    "\u{25BA} \u{266a} QUEUE ({count}) \u{25C4}".into()
}
fn default_title_lyrics() -> String {
    "\u{25BA} \u{266a} LYRICS \u{25C4}".into()
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
