use y7dl::{Client, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::new();

    // Accepts watch/youtu.be/embed/shorts URLs or a bare 11-char ID.
    let video = client
        .get_video("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        .await?;

    println!(
        "{} by {} ({}s)",
        video.title,
        video.author,
        video.duration.as_secs()
    );

    // Inspect available formats.
    for format in &video.formats {
        println!(
            "itag {:>3}  {:<8}  {}",
            format.itag,
            format.quality_label.as_deref().unwrap_or("-"),
            format.mime_type,
        );
    }

    // Pick a format: by itag, by quality, or best available.
    let format = video
        .formats_with_quality("720p")
        .into_iter()
        .next()
        .or_else(|| video.best_video())
        .expect("no formats");

    // Download it.
    let bytes = client.download_to_file(&video, format, "video.mp4").await?;
    println!("downloaded {bytes} bytes");
    Ok(())
}
