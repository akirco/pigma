use crate::error::Result;
use crate::model::{SearchQuery, SearchResult, SonarSource, Song};
use crate::provider::{
    SonarProvider, bilivideo::BiliVideoProvider, kugou::KugouProvider, kuwo::KuwoProvider,
    youtube::YoutubeProvider,
};
use std::sync::Arc;
use tokio::sync::mpsc;

use once_cell::sync::Lazy;
use regex::Regex;

/// How [`SonarFinder`] ranks the combined results from all providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// Return results in the order providers responded (cheapest, ignores match quality).
    FirstReturned,
    /// Re-rank by [`SonarFinder::calculate_match_score`] so the closest title wins (default).
    BestScore,
}

/// Weight multiplier for a query token found in the artist field (stronger
/// signal than a bare name hit, see [`SonarFinder::calculate_match_score`]).
const ARTIST_HIT_WEIGHT: f64 = 1.5;

/// Duration-match bonus tiers, in milliseconds of difference from the target.
/// A hit within 3s is worth more than a loose 30s match.
const DURATION_MATCH_MS: [(u64, f64); 3] = [(3_000, 1.0), (10_000, 0.5), (30_000, 0.25)];

/// Score deducted from candidates whose title marks them as a secondary
/// version of the track — an instrumental/accompaniment (伴奏/纯音乐/卡拉OK) or
/// a concert/live recording (演唱会/现场/live). Present so the original studio
/// recording wins ties; these versions often share the exact same title tokens
/// and duration as the real track, so a bare token score cannot tell them
/// apart.
const SECONDARY_VERSION_PENALTY: f64 = 1.5;

/// Literal markers (already in lowercase) that flag a non-original version.
const SECONDARY_VERSION_MARKERS: &[&str] = &[
    "伴奏",
    "纯音乐",
    "无人声",
    "无和声",
    "karaoke",
    "卡拉ok",
    "ktv",
    "instrumental",
    "演唱会",
    "现场",
];

/// A generic "instrument remake" pattern: a musical instrument followed by a
/// version/piece suffix (钢琴版 / 吉他曲 / 小提琴独奏...), which almost always
/// denotes an instrumental cover. `[板块]` tolerates the common typo.
static INSTRUMENTAL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:钢琴|电钢琴|电子琴|吉他|尤克里里|古筝|琵琶|二胡|小提琴|大提琴|笛子|长笛|陶笛|口琴|萨克斯)(?:版|板|曲|独奏|纯音乐)",
    )
    .expect("valid regex")
});

/// A "live" token at a word boundary in lowercase-normalized text (live, live
/// at, live version...). Word boundaries avoid false hits on words that merely
/// contain "live" (e.g. "alive").
static LIVE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\blive\b").expect("valid regex"));

/// Whether an already-normalized title/query carries a secondary-version marker
/// (instrumental/accompaniment, concert or live recording).
fn is_secondary_version(s: &str) -> bool {
    SECONDARY_VERSION_MARKERS.iter().any(|m| s.contains(m))
        || INSTRUMENTAL_RE.is_match(s)
        || LIVE_RE.is_match(s)
}

/// Tunables for a [`SonarFinder`] search session.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Ranking strategy (see [`SearchMode`]).
    pub mode: SearchMode,
    /// Which providers to query, in the given order (priority still re-sorts them).
    pub providers: Vec<SonarSource>,
    /// Allow lossless (flac/sq) candidates where a provider supports them.
    pub enable_flac: bool,
    /// Per-provider search deadline in milliseconds.
    pub timeout_ms: u64,
    /// Cap on songs kept from each provider before merging/ranking.
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
    /// Build a config with the default providers and settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the ranking mode.
    pub fn with_mode(mut self, mode: SearchMode) -> Self {
        self.mode = mode;
        self
    }

    /// Restrict the active providers to the given set.
    pub fn with_providers(mut self, providers: Vec<SonarSource>) -> Self {
        self.providers = providers;
        self
    }

    /// Toggle lossless candidates.
    pub fn with_flac(mut self, enable: bool) -> Self {
        self.enable_flac = enable;
        self
    }

    /// Set the per-provider search timeout in milliseconds.
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

/// Aggregates the configured providers and runs searches across them
/// concurrently, merging and ranking the combined results.
pub struct SonarFinder {
    providers: Vec<Arc<dyn SonarProvider>>,
    config: SearchConfig,
}

impl SonarFinder {
    /// Build a finder from `config`, instantiating and sorting the selected
    /// providers by priority (highest first). Providers that report
    /// [`SonarProvider::enabled`] `false` are skipped.
    pub fn new(config: SearchConfig) -> Result<Self> {
        let mut providers: Vec<Arc<dyn SonarProvider>> = Vec::new();

        for source in &config.providers {
            let provider: Arc<dyn SonarProvider> = match source {
                SonarSource::Kugou => Arc::new(KugouProvider::with_proxy(
                    config.enable_flac,
                    &config.search_proxy,
                )?),
                SonarSource::Kuwo => Arc::new(KuwoProvider::with_proxy(&config.search_proxy)?),
                SonarSource::BiliVideo => {
                    Arc::new(BiliVideoProvider::with_proxy(&config.search_proxy)?)
                }
                SonarSource::Youtube => {
                    Arc::new(YoutubeProvider::with_proxy(&config.youtube_proxy)?)
                }
            };
            if provider.enabled() {
                providers.push(provider);
            }
        }

        providers.sort_by_key(|p| std::cmp::Reverse(p.priority()));

        Ok(Self { providers, config })
    }

    /// The provider sources, ordered by priority (highest first).
    pub fn sources(&self) -> Vec<SonarSource> {
        self.providers.iter().map(|p| p.source()).collect()
    }

    /// Run a search across all providers (each bounded by `timeout_ms`), merge
    /// and rank the results, and return the combined [`SearchResult`]. Returns
    /// [`crate::error::SonarError::NoResults`] when no provider returned anything.
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
        score += artist_hits * ARTIST_HIT_WEIGHT;

        if let Some(target_ms) = query.duration {
            let diff_ms = song.duration.abs_diff(target_ms);
            for (max_diff_ms, bonus) in DURATION_MATCH_MS {
                if diff_ms <= max_diff_ms {
                    score += bonus;
                    break;
                }
            }
        }

        // Demote secondary versions (instrumentals/accompaniments, concert and
        // live recordings) so the original studio recording wins when both
        // surface in the results. Skipped when the search itself asks for such
        // a version (the query carries the marker).
        let query_text = crate::util::normalize_for_match(&query.keyword);
        if !is_secondary_version(&query_text) && is_secondary_version(&name) {
            score -= SECONDARY_VERSION_PENALTY;
        }

        score
    }

    /// Search, then try each ranked song's provider for a playable URL and
    /// return the first that resolves (best quality per the provider). Convenience
    /// wrapper around [`Self::search`] + [`Self::get_play_url_for_song`].
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
    /// and reuse its lyrics. Returns `None` when no lyrics could be found.
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
        None
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

/// One-shot search using the default config and [`SearchMode::BestScore`].
pub async fn quick_search(keyword: &str) -> Result<(Song, crate::model::PlayUrlResult)> {
    let finder = SonarFinder::new(SearchConfig::default())?;
    finder
        .search_and_get_url(&SearchQuery::new(keyword), None)
        .await
}

/// One-shot search using the default config with an explicit [`SearchMode`].
pub async fn quick_search_with_mode(
    keyword: &str,
    mode: SearchMode,
) -> Result<(Song, crate::model::PlayUrlResult)> {
    let config = SearchConfig::new().with_mode(mode);
    let finder = SonarFinder::new(config)?;
    finder
        .search_and_get_url(&SearchQuery::new(keyword), None)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::make_song_id;

    fn song(name: &str, singer: &str) -> Song {
        Song {
            id: make_song_id(SonarSource::Kugou, name),
            source_id: name.to_string(),
            name: name.to_string(),
            singer: singer.to_string(),
            album: String::new(),
            duration: 0,
            source: SonarSource::Kugou,
            pic_url: String::new(),
            meta: Default::default(),
        }
    }

    fn score(name: &str, singer: &str, keyword: &str) -> f64 {
        let finder = SonarFinder::new(SearchConfig::default()).unwrap();
        finder.calculate_match_score(&song(name, singer), &SearchQuery::new(keyword))
    }

    #[test]
    fn vocal_beats_instrumental() {
        let vocal = score("晴天", "周杰伦", "晴天 周杰伦");
        let instrumental = score("晴天 伴奏", "周杰伦", "晴天 周杰伦");
        assert!(
            vocal > instrumental,
            "vocal {vocal} should outscore 伴奏 {instrumental}"
        );
    }

    #[test]
    fn instrumental_query_not_penalised() {
        // When the user explicitly searches for an instrumental, its token hits
        // should count normally instead of being penalised.
        let q = "晴天 伴奏";
        let vocal = score("晴天", "周杰伦", q);
        let instrumental = score("晴天 伴奏", "周杰伦", q);
        assert!(
            instrumental > vocal,
            "explicit 伴奏 query should rank it first"
        );
    }

    #[test]
    fn piano_remake_is_penalised() {
        let vocal = score("晴天", "周杰伦", "晴天 周杰伦");
        let remake = score("晴天 钢琴版", "周杰伦", "晴天 周杰伦");
        assert!(
            vocal > remake,
            "vocal {vocal} should outscore 钢琴版 {remake}"
        );
    }

    #[test]
    fn concert_and_live_are_penalised() {
        let vocal = score("晴天", "周杰伦", "晴天 周杰伦");
        for title in [
            "晴天 演唱会",
            "晴天(现场版)",
            "晴天 live",
            "晴天 live at 演唱会",
        ] {
            let version = score(title, "周杰伦", "晴天 周杰伦");
            assert!(
                vocal > version,
                "vocal {vocal} should outscore {title:?} ({version})"
            );
        }
    }

    #[test]
    fn concert_query_not_penalised() {
        let q = "晴天 演唱会";
        let vocal = score("晴天", "周杰伦", q);
        let concert = score("晴天 演唱会", "周杰伦", q);
        assert!(
            concert > vocal,
            "explicit 演唱会 query should rank the live version first"
        );
    }

    #[test]
    fn secondary_version_markers_recognised() {
        assert!(is_secondary_version("晴天 (karaoke)"));
        assert!(is_secondary_version("晴天 ktv"));
        assert!(is_secondary_version("晴天 纯音乐"));
        assert!(is_secondary_version("晴天 演唱会"));
        assert!(is_secondary_version("晴天 现场版"));
        assert!(is_secondary_version("晴天 live"));
        assert!(!is_secondary_version("晴天"));
        assert!(
            !is_secondary_version("alive"),
            "word boundary must guard live"
        );
    }
}
