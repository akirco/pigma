use sonar::{SearchConfig, SearchQuery, SonarFinder, SonarSource};

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let finder = SonarFinder::new(
        SearchConfig::new()
            .with_providers(vec![SonarSource::BiliVideo])
            .with_timeout(15000),
    );
    let result = finder
        .search(&SearchQuery::new("只有爱 许巍"))
        .await
        .unwrap();
    println!("{:?}", result);
}
