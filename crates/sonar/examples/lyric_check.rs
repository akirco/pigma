use sonar::{SearchConfig, SearchQuery, SonarFinder, SonarSource};

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let finder = SonarFinder::new(
        SearchConfig::new()
            .with_providers(vec![SonarSource::Kuwo])
            .with_timeout(15000),
    );
    let result = finder
        .search(&SearchQuery::new("晴天 周杰伦"))
        .await
        .unwrap();
    for (i, song) in result.songs.iter().enumerate() {
        let lrc = finder.get_lyrics(song).await.ok().flatten();
        println!(
            "#{i} [{:?}] {} - {} pic={} lyrics={}",
            song.source,
            song.name,
            song.singer,
            !song.pic_url.is_empty(),
            lrc.as_ref().map(|l| l.lines().count()).unwrap_or(0)
        );
    }
}
