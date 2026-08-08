use std::sync::Arc;

use crate::text_input::TextInput;

/// A search backend selectable in the search bar. `Ncm` is the default and
/// searches NetEase Cloud Music; the rest delegate to sonar providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchProvider {
    #[default]
    Ncm,
    Kugou,
    Kuwo,
    BiliVideo,
    Youtube,
}

impl SearchProvider {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Ncm => "netease",
            Self::Kugou => "kugou",
            Self::Kuwo => "kuwo",
            Self::BiliVideo => "bilivideo",
            Self::Youtube => "youtube",
        }
    }

    pub fn to_sonar(self) -> Option<sonar::SonarSource> {
        match self {
            Self::Ncm => None,
            Self::Kugou => Some(sonar::SonarSource::Kugou),
            Self::Kuwo => Some(sonar::SonarSource::Kuwo),
            Self::BiliVideo => Some(sonar::SonarSource::BiliVideo),
            Self::Youtube => Some(sonar::SonarSource::Youtube),
        }
    }

    pub fn from_sonar(source: sonar::SonarSource) -> Self {
        match source {
            sonar::SonarSource::Kugou => Self::Kugou,
            sonar::SonarSource::Kuwo => Self::Kuwo,
            sonar::SonarSource::BiliVideo => Self::BiliVideo,
            sonar::SonarSource::Youtube => Self::Youtube,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub active: bool,
    pub input: TextInput,
    pub filter_queue_only: bool,
    pub unfiltered_songs: Option<Vec<Arc<ncm_api::SongInfo>>>,
    /// Available providers in display order (网易云 first, then configured
    /// sonar sources). Populated from config at startup.
    pub providers: Vec<SearchProvider>,
    /// Currently selected search provider.
    pub provider: SearchProvider,
}

impl SearchState {
    pub fn cycle_provider(&mut self, forward: bool) {
        if self.providers.is_empty() {
            return;
        }
        let idx = self
            .providers
            .iter()
            .position(|p| *p == self.provider)
            .unwrap_or(0);
        let len = self.providers.len();
        self.provider = if forward {
            self.providers[(idx + 1) % len]
        } else {
            self.providers[(idx + len - 1) % len]
        };
    }
}
