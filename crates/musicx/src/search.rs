use crate::error::Result;
use crate::model::{MusicSource, SearchQuery, SearchResult, Song};
use crate::provider::{
    MusicProvider, bilivideo::BiliVideoProvider, kugou::KugouProvider, kuwo::KuwoProvider,
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
    pub providers: Vec<MusicSource>,
    pub enable_flac: bool,
    pub timeout_ms: u64,
    pub max_results_per_provider: usize,
    /// Proxy URL for the YouTube provider (empty = direct).
    pub proxy: String,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            mode: SearchMode::BestScore,
            providers: vec![
                MusicSource::Kugou,
                MusicSource::Kuwo,
                MusicSource::BiliVideo,
                MusicSource::Youtube,
            ],
            enable_flac: true,
            timeout_ms: 10000,
            max_results_per_provider: 10,
            proxy: String::new(),
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

    pub fn with_providers(mut self, providers: Vec<MusicSource>) -> Self {
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

    pub fn with_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.proxy = proxy.into();
        self
    }
}

pub struct MusicFinder {
    providers: Vec<Arc<dyn MusicProvider>>,
    config: SearchConfig,
}

impl MusicFinder {
    pub fn new(config: SearchConfig) -> Self {
        let mut providers: Vec<Arc<dyn MusicProvider>> = Vec::new();

        for source in &config.providers {
            let provider: Arc<dyn MusicProvider> = match source {
                MusicSource::Kugou => Arc::new(KugouProvider::new(config.enable_flac)),
                MusicSource::Kuwo => Arc::new(KuwoProvider::new(config.enable_flac)),
                MusicSource::BiliVideo => Arc::new(BiliVideoProvider::new()),
                MusicSource::Youtube => Arc::new(YoutubeProvider::with_proxy(&config.proxy)),
            };
            if provider.enabled() {
                providers.push(provider);
            }
        }

        providers.sort_by_key(|p| std::cmp::Reverse(p.priority()));

        Self { providers, config }
    }

    pub async fn search(&self, query: &SearchQuery) -> Result<SearchResult> {
        let (tx, mut rx) = mpsc::channel(self.providers.len());
        let query = query.clone();

        for provider in &self.providers {
            let provider = provider.clone();
            let tx = tx.clone();
            let query = query.clone();
            let timeout = self.config.timeout_ms;

            tokio::spawn(async move {
                let result = tokio::time::timeout(
                    std::time::Duration::from_millis(timeout),
                    provider.search(&query),
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
            return Err(crate::error::MusicError::NoResults);
        }

        let combined = self.merge_results(all_results, &query);
        Ok(combined)
    }

    fn merge_results(&self, results: Vec<SearchResult>, query: &SearchQuery) -> SearchResult {
        let mut all_songs = Vec::new();
        for mut result in results {
            for song in &mut result.songs {
                if all_songs.len() >= self.config.max_results_per_provider * self.providers.len() {
                    break;
                }
                all_songs.push(song.clone());
            }
        }

        let scored_songs = self.score_songs(all_songs, query);

        let final_songs = match self.config.mode {
            SearchMode::FirstReturned => scored_songs,
            SearchMode::BestScore => {
                let mut sorted = scored_songs;
                sorted.sort_by(|a, b| {
                    let score_b = b
                        .raw_data
                        .get("match_score")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let score_a = a
                        .raw_data
                        .get("match_score")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    score_b
                        .partial_cmp(&score_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                sorted
            }
        };

        SearchResult {
            songs: final_songs,
            source: MusicSource::Kugou,
            query: query.clone(),
            total: None,
        }
    }

    fn score_songs(&self, songs: Vec<Song>, query: &SearchQuery) -> Vec<Song> {
        songs
            .into_iter()
            .map(|mut song| {
                let score = self.calculate_match_score(&song, query);
                song.raw_data["match_score"] = serde_json::Value::Number(
                    serde_json::Number::from_f64(score).unwrap_or(serde_json::Number::from(0)),
                );
                song
            })
            .collect()
    }

    /// Score how well a song matches the query, based on the artist, the song
    /// name and (optionally) the duration. The final selection picks the best
    /// scoring source that can actually provide a playable URL.
    fn calculate_match_score(&self, song: &Song, query: &SearchQuery) -> f64 {
        let mut score = 0.0;

        let name = crate::util::normalize_for_match(&song.name);
        let artist = song
            .artists
            .iter()
            .map(|a| crate::util::normalize_for_match(&a.name))
            .collect::<Vec<_>>()
            .join(" ");

        for token in query.keyword.to_lowercase().split_whitespace() {
            if !token.is_empty() {
                let token = crate::util::normalize_cjk(token);
                if name.contains(&token) {
                    score += 1.0;
                }
                if artist.contains(&token) {
                    score += 1.0;
                }
            }
        }

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

        let provider_for = |source: MusicSource| {
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

        Err(crate::error::MusicError::NoPlayUrl)
    }
}

impl Default for MusicFinder {
    fn default() -> Self {
        Self::new(SearchConfig::default())
    }
}

pub async fn quick_search(keyword: &str) -> Result<(Song, crate::model::PlayUrlResult)> {
    let finder = MusicFinder::default();
    finder
        .search_and_get_url(&SearchQuery::new(keyword), None)
        .await
}

pub async fn quick_search_with_mode(
    keyword: &str,
    mode: SearchMode,
) -> Result<(Song, crate::model::PlayUrlResult)> {
    let config = SearchConfig::new().with_mode(mode);
    let finder = MusicFinder::new(config);
    finder
        .search_and_get_url(&SearchQuery::new(keyword), None)
        .await
}
