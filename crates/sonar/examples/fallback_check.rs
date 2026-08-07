use sonar::{SearchConfig, SearchQuery, SonarFinder, SonarSource};

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let finder = SonarFinder::new(SearchConfig::default().with_timeout(15000));

    for src in [
        SonarSource::Kugou,
        SonarSource::Kuwo,
        SonarSource::BiliVideo,
        SonarSource::Youtube,
    ] {
        let result = finder
            .search(&SearchQuery::new("晴天 周杰伦"))
            .await
            .unwrap();
        let song = result.songs.iter().find(|s| s.source == src);
        let Some(song) = song else { continue };
        let lyrics = finder.get_lyrics_fallback(song).await;
        let cover = finder.get_cover_fallback(song).await;
        println!(
            "[{:?}] {} - {}  lyrics={}  cover={}",
            src,
            song.name,
            song.singer,
            lyrics.as_ref().map(|l| l.lines().count()).unwrap_or(0),
            cover.as_ref().map(|c| c.len()).unwrap_or(0)
        );
    }
}
