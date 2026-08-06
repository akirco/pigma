use crate::error::Result;
use crate::model::{SearchQuery, SearchResult, SonarSource, Song};
use crate::provider::{
    SonarProvider, bilivideo::BiliVideoProvider, kugou::KugouProvider, kuwo::KuwoProvider,
    youtube::YoutubeProvider,
};
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    FirstReturned,
    BestScore,
}

#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub mode: SearchMode,
    pub providers: Vec<SonarSource>,
    pub enable_flac: bool,
    pub timeout_ms: u64,
    pub max_results_per_provider: usize,
    /// Proxy URL for the domestic providers (kugou, kuwo, bilivideo). Empty = direct.
    pub search_proxy: String,
    /// Proxy URL for the YouTube provider. Empty = direct.
    pub youtube_proxy: String,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            mode: SearchMode::BestScore,
            providers: vec![
                SonarSource::Kugou,
                SonarSource::Kuwo,
                SonarSource::BiliVideo,
                SonarSource::Youtube,
            ],
            enable_flac: true,
            timeout_ms: 10000,
            max_results_per_provider: 30,
            search_proxy: String::new(),
            youtube_proxy: String::new(),
        }
    }
}

impl SearchConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mode(mut self, mode: SearchMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_providers(mut self, providers: Vec<SonarSource>) -> Self {
        self.providers = providers;
        self
    }

    pub fn with_flac(mut self, enable: bool) -> Self {
        self.enable_flac = enable;
        self
    }

    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    /// Proxy URL for the domestic providers (kugou, kuwo, bilivideo).
    pub fn with_search_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.search_proxy = proxy.into();
        self
    }

    /// Proxy URL for the YouTube provider.
    pub fn with_youtube_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.youtube_proxy = proxy.into();
        self
    }

    /// Deprecated: alias for [`Self::with_youtube_proxy`].
    pub fn with_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.youtube_proxy = proxy.into();
        self
    }
}

pub struct SonarFinder {
    providers: Vec<Arc<dyn SonarProvider>>,
    config: SearchConfig,
}

impl SonarFinder {
    pub fn new(config: SearchConfig) -> Self {
        let mut providers: Vec<Arc<dyn SonarProvider>> = Vec::new();

        for source in &config.providers {
            let provider: Arc<dyn SonarProvider> = match source {
                SonarSource::Kugou => Arc::new(KugouProvider::with_proxy(
                    config.enable_flac,
                    &config.search_proxy,
                )),
                SonarSource::Kuwo => Arc::new(KuwoProvider::with_proxy(&config.search_proxy)),
                SonarSource::BiliVideo => {
                    Arc::new(BiliVideoProvider::with_proxy(&config.search_proxy))
                }
                SonarSource::Youtube => {
                    Arc::new(YoutubeProvider::with_proxy(&config.youtube_proxy))
                }
            };
            if provider.enabled() {
                providers.push(provider);
            }
        }

        providers.sort_by_key(|p| std::cmp::Reverse(p.priority()));

        Self { providers, config }
    }

    /// The provider sources, ordered by priority (highest first).
    pub fn sources(&self) -> Vec<SonarSource> {
        self.providers.iter().map(|p| p.source()).collect()
    }

    pub async fn search(&self, query: &SearchQuery) -> Result<SearchResult> {
        let (tx, mut rx) = mpsc::channel(self.providers.len());
        let query = std::sync::Arc::new(query.clone());

        for provider in &self.providers {
            let provider = provider.clone();
            let tx = tx.clone();
            let query = std::sync::Arc::clone(&query);
            let timeout = self.config.timeout_ms;

            tokio::spawn(async move {
                let result = tokio::time::timeout(
                    std::time::Duration::from_millis(timeout),
                    provider.search(query.as_ref()),
                )
                .await;

                let _ = tx.send((provider.source(), result)).await;
            });
        }
        drop(tx);

        let mut all_results = Vec::new();
        while let Some((source, result)) = rx.recv().await {
            match result {
                Ok(Ok(search_result)) => {
                    all_results.push(search_result);
                }
                Ok(Err(e)) => {
                    log::warn!("Provider {:?} search failed: {}", source, e);
                }
                Err(_) => {
                    log::warn!("Provider {:?} search timed out", source);
                }
            }
        }

        if all_results.is_empty() {
            return Err(crate::error::SonarError::NoResults);
        }

        let combined = self.merge_results(all_results, query.as_ref());
        Ok(combined)
    }

    fn merge_results(&self, results: Vec<SearchResult>, query: &SearchQuery) -> SearchResult {
        let mut all_songs = Vec::new();
        for result in results {
            for song in result.songs {
                if all_songs.len() >= self.config.max_results_per_provider * self.providers.len() {
                    break;
                }
                all_songs.push(song);
            }
        }

        // Tie-break equal scores by provider priority so the final ranking is
        // deterministic instead of depending on mpsc arrival order.
        let priority_of = |source: SonarSource| {
            self.providers
                .iter()
                .position(|p| p.source() == source)
                .unwrap_or(usize::MAX)
        };

        let final_songs = match self.config.mode {
            SearchMode::FirstReturned => all_songs,
            SearchMode::BestScore => {
                let mut scored: Vec<(f64, Song)> = all_songs
                    .into_iter()
                    .map(|song| (self.calculate_match_score(&song, query), song))
                    .collect();
                scored.sort_by(|a, b| {
                    b.0.partial_cmp(&a.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| priority_of(a.1.source).cmp(&priority_of(b.1.source)))
                });
                scored.into_iter().map(|(_, song)| song).collect()
            }
        };

        SearchResult {
            songs: final_songs,
            source: SonarSource::Kugou,
            query: query.clone(),
            total: None,
        }
    }

    /// Score how well a song matches the query, based on the artist, the song
    /// name and (optionally) the duration. The final selection picks the best
    /// scoring source that can actually provide a playable URL.
    fn calculate_match_score(&self, song: &Song, query: &SearchQuery) -> f64 {
        let mut score = 0.0;

        let name = crate::util::normalize_for_match(&song.name);
        let artist = crate::util::normalize_for_match(&song.singer);

        let mut name_hits = 0.0;
        let mut artist_hits = 0.0;

        for token in query.keyword.to_lowercase().split_whitespace() {
            if token.is_empty() {
                continue;
            }
            let token = crate::util::normalize_cjk(token);
            if name.contains(&token) {
                name_hits += 1.0;
            }
            if artist.contains(&token) {
                artist_hits += 1.0;
            }
        }

        // A token credited to the artist field is a much stronger signal than
        // merely appearing in a cover/live/lyrics title. Weighting artist hits
        // higher stops titles that embed the artist name from outscoring the
        // real recording (e.g. "只有爱 (cover: 许巍)" vs "只有爱 - 许巍").
        score += name_hits;
        score += artist_hits * 1.5;

        if let Some(target_ms) = query.duration {
            let diff_ms = song.duration.abs_diff(target_ms);
            if diff_ms <= 3_000 {
                score += 1.0;
            } else if diff_ms <= 10_000 {
                score += 0.5;
            } else if diff_ms <= 30_000 {
                score += 0.25;
            }
        }

        score
    }

    pub async fn search_and_get_url(
        &self,
        query: &SearchQuery,
        quality: Option<crate::model::Quality>,
    ) -> Result<(Song, crate::model::PlayUrlResult)> {
        let result = self.search(query).await?;

        let provider_for = |source: SonarSource| {
            self.providers
                .iter()
                .find(|p| p.source() == source)
                .cloned()
        };

        for song in result.songs {
            let provider = match provider_for(song.source) {
                Some(p) => p,
                None => continue,
            };
            let timeout = std::time::Duration::from_millis(self.config.timeout_ms);
            match tokio::time::timeout(timeout, provider.get_play_url(&song, quality)).await {
                Ok(Ok(play_url)) => return Ok((song, play_url)),
                Ok(Err(e)) => {
                    log::debug!("source {:?} for {} failed: {}", song.source, song.name, e)
                }
                Err(_) => log::debug!("source {:?} for {} timed out", song.source, song.name),
            }
        }

        Err(crate::error::SonarError::NoPlayUrl)
    }

    /// Resolve a play URL for a specific song directly via the provider that
    /// produced it (no keyword re-search).
    pub async fn get_play_url_for_song(
        &self,
        song: &crate::model::Song,
        quality: Option<crate::model::Quality>,
    ) -> Result<crate::model::PlayUrlResult> {
        let provider = self
            .providers
            .iter()
            .find(|p| p.source() == song.source)
            .cloned()
            .ok_or(crate::error::SonarError::NoPlayUrl)?;
        let timeout = std::time::Duration::from_millis(self.config.timeout_ms);
        match tokio::time::timeout(timeout, provider.get_play_url(song, quality)).await {
            Ok(Ok(play_url)) => Ok(play_url),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(crate::error::SonarError::Timeout),
        }
    }

    /// Fetch LRC lyrics for a song via the provider that produced it.
    pub async fn get_lyrics(&self, song: &crate::model::Song) -> Result<Option<String>> {
        let provider = self.providers.iter().find(|p| p.source() == song.source);
        match provider {
            Some(p) => p.get_lyrics(song).await,
            None => Ok(None),
        }
    }

    /// Best-effort lyrics: the song's own provider first, then keyword-search
    /// the configured sources (kugou preferred, then kuwo) for a matching song
    /// and reuse its lyrics.
    pub async fn get_lyrics_fallback(&self, song: &crate::model::Song) -> Option<String> {
        if let Ok(Some(l)) = self.get_lyrics(song).await
            && !l.trim().is_empty()
        {
            return Some(l);
        }
        for source in [SonarSource::Kugou, SonarSource::Kuwo] {
            let candidate = self.search_first(&[source], song).await?;
            if let Ok(Some(l)) = self.get_lyrics(&candidate).await
                && !l.trim().is_empty()
            {
                return Some(l);
            }
        }
        Some("未找到歌词".into())
    }

    /// Best-effort cover: the song's own cover first, else keyword-search the
    /// configured sources (kuwo preferred, which provides album covers) and
    /// reuse the match's cover.
    pub async fn get_cover_fallback(&self, song: &crate::model::Song) -> Option<String> {
        if !song.pic_url.is_empty() {
            return Some(song.pic_url.clone());
        }
        for source in [
            SonarSource::Kuwo,
            SonarSource::Kugou,
            SonarSource::BiliVideo,
        ] {
            let candidate = self.search_first(&[source], song).await?;
            if !candidate.pic_url.is_empty() {
                return Some(candidate.pic_url);
            }
        }
        None
    }

    /// Search a set of providers for the first song matching `song` by keyword.
    async fn search_first(
        &self,
        sources: &[SonarSource],
        song: &crate::model::Song,
    ) -> Option<Song> {
        let query =
            SearchQuery::new(format!("{} {}", song.name, song.singer)).with_duration(song.duration);
        for source in sources {
            let provider = self
                .providers
                .iter()
                .find(|p| p.source() == *source)
                .cloned()?;
            if let Ok(result) = provider.search(&query).await
                && let Some(first) = result.songs.into_iter().next()
            {
                return Some(first);
            }
        }
        None
    }
}

impl Default for SonarFinder {
    fn default() -> Self {
        Self::new(SearchConfig::default())
    }
}

pub async fn quick_search(keyword: &str) -> Result<(Song, crate::model::PlayUrlResult)> {
    let finder = SonarFinder::default();
    finder
        .search_and_get_url(&SearchQuery::new(keyword), None)
        .await
}

pub async fn quick_search_with_mode(
    keyword: &str,
    mode: SearchMode,
) -> Result<(Song, crate::model::PlayUrlResult)> {
    let config = SearchConfig::new().with_mode(mode);
    let finder = SonarFinder::new(config);
    finder
        .search_and_get_url(&SearchQuery::new(keyword), None)
        .await
}
