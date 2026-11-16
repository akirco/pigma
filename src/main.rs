//! Process entry point: installs the async runtime and hands off to the
//! library's bootstrap (`pigma::run`).

#[tokio::main]
async fn main() -> eyre::Result<()> {
    pigma::run().await
}
