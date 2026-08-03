use musicx::{MusicFinder, MusicSource, Quality, SearchConfig, SearchQuery};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let proxy = std::env::var("MUSICX_PROXY").unwrap_or_default();
    let config = SearchConfig::new()
        .with_providers(vec![
            MusicSource::Kuwo,
            MusicSource::Kugou,
            MusicSource::BiliVideo,
            MusicSource::Youtube,
        ])
        .with_timeout(12000)
        .with_proxy(proxy);
    let finder = MusicFinder::new(config);

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
            println!("  source:    {}", found.source);
            println!("  name:      {}", found.name);
            println!(
                "  artists:   {}",
                found
                    .artists
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            println!(
                "  duration:  {} ms (target {duration_ms} ms)",
                found.duration
            );
            println!("  quality:   {}", play.quality);
            println!("  url:       {}", play.url);
            let score = found
                .raw_data
                .get("match_score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            println!("  score:     {score}");
        }
        Err(e) => {
            println!("FALLBACK FAILED: {e}");
        }
    }
    Ok(())
}
