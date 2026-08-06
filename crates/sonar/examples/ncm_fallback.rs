use sonar::{Quality, SearchConfig, SearchQuery, SonarFinder, SonarSource};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let proxy = std::env::var("SONAR_PROXY").unwrap_or_default();
    let config = SearchConfig::new()
        .with_providers(vec![
            SonarSource::Kuwo,
            SonarSource::Kugou,
            SonarSource::BiliVideo,
            SonarSource::Youtube,
        ])
        .with_timeout(12000)
        .with_proxy(proxy);
    let finder = SonarFinder::new(config);

    let args: Vec<String> = std::env::args().collect();
    let (name, singer, duration_ms) = if args.len() >= 3 {
        (
            args[1].clone(),
            args[2].clone(),
            args.get(3).and_then(|d| d.parse().ok()).unwrap_or(0),
        )
    } else {
        ("晴天".to_string(), "周杰伦".to_string(), 269000)
    };

    let keyword = format!("{name} {singer}");
    let query = SearchQuery::new(keyword).with_duration(duration_ms);

    match finder.search_and_get_url(&query, Some(Quality::High)).await {
        Ok((found, play)) => {
            println!("FALLBACK OK");
            println!("source:    {}", found.source);
            println!("name:      {}", found.name);
            println!("artists:   {}", found.singer);
            println!("duration:  {} ms (target {duration_ms} ms)", found.duration);
            println!("quality:   {}", play.quality);
            println!("url:       {}", play.url);
        }
        Err(e) => {
            println!("FALLBACK FAILED: {e}");
        }
    }
    Ok(())
}
