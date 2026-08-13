use super::NcmClient;
use crate::error::NcmError;

impl NcmClient {
    // ===== Download =====

    /// Download an image from the network to a local path
    ///
    /// * `url` — image URL
    /// * `path` — local save path (including the file name)
    /// * `width` — requested width
    /// * `height` — requested height
    pub async fn download_img(
        &self,
        url: &str,
        path: std::path::PathBuf,
        width: u16,
        height: u16,
    ) -> Result<(), NcmError> {
        if path.exists() {
            return Ok(());
        }
        let image_url = format!("{}?param={}y{}", url, width, height);
        let bytes = self.http.get(&image_url).send().await?.bytes().await?;
        let parent = path.parent().map(|p| p.to_path_buf());
        tokio::task::spawn_blocking(move || {
            if let Some(parent) = parent {
                if let Err(e) = std::fs::create_dir_all(&parent) {
                    log::warn!("failed to create image dir {:?}: {}", parent, e);
                }
            }
            std::fs::write(&path, &bytes)
        })
        .await
        .map_err(|e| NcmError::Session(format!("spawn_blocking failed: {e}")))?
        .map_err(|e| NcmError::Session(format!("failed to write image: {e}")))?;
        Ok(())
    }

    /// Download a song from the network to a local path
    ///
    /// * `url` — song URL
    /// * `path` — local save path (including the file name)
    pub async fn download_song(&self, url: &str, path: std::path::PathBuf) -> Result<(), NcmError> {
        if path.exists() {
            return Ok(());
        }
        let bytes = self.http.get(url).send().await?.bytes().await?;
        let parent = path.parent().map(|p| p.to_path_buf());
        tokio::task::spawn_blocking(move || {
            if let Some(parent) = parent {
                if let Err(e) = std::fs::create_dir_all(&parent) {
                    log::warn!("failed to create song dir {:?}: {}", parent, e);
                }
            }
            std::fs::write(&path, &bytes)
        })
        .await
        .map_err(|e| NcmError::Session(format!("spawn_blocking failed: {e}")))?
        .map_err(|e| NcmError::Session(format!("failed to write song: {e}")))?;
        Ok(())
    }
}
