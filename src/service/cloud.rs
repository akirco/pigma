//! Cloud disk: uploading local audio files with their metadata.

use std::path::Path;

use super::ApiService;

impl ApiService {
    pub async fn upload_song_with_meta(
        &self,
        path: &Path,
        song_hint: &str,
        album_hint: &str,
        artist_hint: &str,
    ) -> Result<ncm_api::CloudUploadResult, ncm_api::NcmError> {
        self.client
            .upload_song_with_meta(path, song_hint, album_hint, artist_hint)
            .await
    }
}
