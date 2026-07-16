use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ncm_api::{NcmClient, SongInfo};
use stream_download::{Settings, StreamDownload};

use super::player::{Input, SharedReader};
use crate::cache::CacheManager;

#[derive(Clone)]
pub struct AudioSource {
    api: Arc<NcmClient>,
    pub cache: CacheManager,
}

impl AudioSource {
    pub fn new(api: Arc<NcmClient>, cache_dir: PathBuf) -> Self {
        Self {
            api,
            cache: CacheManager::with_cache_dir(cache_dir),
        }
    }

    pub async fn resolve(&self, song: &SongInfo) -> Result<Input, String> {
        if song.id == 0 {
            let path = std::path::Path::new(&song.album);
            let file = std::fs::File::open(path).map_err(|e| {
                format!("\u{65e0}\u{6cd5}\u{6253}\u{5f00}\u{672c}\u{5730}\u{6587}\u{4ef6}: {e}")
            })?;
            return Ok(Input::Reader(SharedReader(Arc::new(Mutex::new(Box::new(
                file,
            ))))));
        }

        let ext = "mp3";

        if self.cache.is_cached(song.id, ext) {
            let file = self.cache.open_cached(song.id, ext).map_err(|e| {
                format!("\u{65e0}\u{6cd5}\u{6253}\u{5f00}\u{7f13}\u{5b58}\u{6587}\u{4ef6}: {e}")
            })?;
            return Ok(Input::Reader(SharedReader(Arc::new(Mutex::new(Box::new(
                file,
            ))))));
        }

        let urls = self
            .api
            .songs_url(&[song.id], "320000")
            .await
            .map_err(|e| format!("\u{83b7}\u{53d6}\u{6b4c}\u{66f2}URL\u{5931}\u{8d25}: {e}"))?;

        let url_str = urls
            .first()
            .filter(|u| !u.url.is_empty())
            .map(|u| &u.url)
            .ok_or_else(|| {
                "\u{8be5}\u{6b4c}\u{66f2}\u{6682}\u{65e0}\u{64ad}\u{653e}\u{6e90}".to_string()
            })?;

        let url = url::Url::parse(url_str)
            .map_err(|e| format!("URL \u{89e3}\u{6790}\u{5931}\u{8d25}: {e}"))?;

        let provider = self
            .cache
            .create_provider(song.id, ext)
            .map_err(|e| format!("\u{7f13}\u{5b58}\u{521b}\u{5efa}\u{5931}\u{8d25}: {e}"))?;

        let reader = StreamDownload::new_http(url, provider, Settings::default())
            .await
            .map_err(|e| format!("\u{6d41}\u{4e0b}\u{8f7d}\u{5931}\u{8d25}: {e}"))?;

        Ok(Input::Reader(SharedReader(Arc::new(Mutex::new(Box::new(
            reader,
        ))))))
    }
}
