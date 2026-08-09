use super::NcmClient;
use crate::error::NcmError;

impl NcmClient {
    // ===== 下载 =====

    /// 从网络下载图片到本地
    ///
    /// * `url` — 图片 URL
    /// * `path` — 本地保存路径（含文件名）
    /// * `width` — 请求宽度
    /// * `height` — 请求高度
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

    /// 从网络下载歌曲到本地
    ///
    /// * `url` — 歌曲 URL
    /// * `path` — 本地保存路径（含文件名）
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
