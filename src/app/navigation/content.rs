//! Breadcrumb/history transitions, cell actions and cached-song upload, i.e.
//! what happens while browsing an already-loaded content page.

use std::path::PathBuf;

use super::super::{App, event::send_event};
use crate::{
    event::{AppEvent, NavigationEvent},
    service::ApiEndpoint,
    state::ContentState,
};

impl App {
    pub(crate) fn handle_breadcrumb(&mut self, name: String, generation: u64) {
        // A stale load must not relabel the page the user already moved to.
        if generation != self.state.navigation.generation {
            return;
        }
        self.state.navigation.nav.subtitle = Some(name);
    }

    /// Restore the previous breadcrumb level (Esc from the content page).
    pub(crate) fn handle_content_restore(&mut self) {
        // Invalidate in-flight loads from the level being left, otherwise a
        // slow response could immediately repopulate the restored page.
        self.bump_content_generation();
        self.state.navigation.pop_breadcrumb();
    }

    pub(crate) fn handle_cell_action(&mut self, row: usize, col: usize) -> eyre::Result<()> {
        let columns = self
            .config
            .columns
            .for_content(self.state.navigation.content.content_type(), None)
            .to_vec();
        let Some(column) = columns.get(col) else {
            return Ok(());
        };
        let field = column.field.as_str();

        match (self.state.navigation.content.as_ref(), field) {
            (ContentState::Songs(songs), "album") => {
                if let Some(song) = songs.get(row) {
                    let album_id = song.album_id;
                    let name = format!("{}: {}", column.header, song.album);
                    self.state.navigation.push_breadcrumb();
                    self.state.navigation.set_content(ContentState::Loading);
                    let generation = self.bump_content_generation();
                    let service = self.service.clone();
                    let sender = self.events.sender();
                    tokio::spawn(async move {
                        let state = service.load_album(album_id).await;
                        let _ = sender.send(
                            NavigationEvent::ContentLoaded {
                                content: state,
                                generation,
                            }
                            .into(),
                        );
                        let _ =
                            sender.send(NavigationEvent::BreadcrumbSet { name, generation }.into());
                    });
                }
            }
            (ContentState::Songs(songs), "singer") => {
                if let Some(song) = songs.get(row) {
                    let artist_id = song.artist_id;
                    if artist_id == 0 {
                        return Ok(());
                    }
                    let name = format!("{}: {}", column.header, song.singer);
                    self.state.navigation.push_breadcrumb();
                    self.state.navigation.set_content(ContentState::Loading);
                    let generation = self.bump_content_generation();
                    let service = self.service.clone();
                    let sender = self.events.sender();
                    let limit = self.config.search_limit;
                    tokio::spawn(async move {
                        let state = service.load_artist_songs(artist_id, limit).await;
                        let _ = sender.send(
                            NavigationEvent::ContentLoaded {
                                content: state,
                                generation,
                            }
                            .into(),
                        );
                        let _ =
                            sender.send(NavigationEvent::BreadcrumbSet { name, generation }.into());
                    });
                }
            }
            (ContentState::Singers(singers), "name") => {
                if let Some(singer) = singers.get(row) {
                    let artist_id = singer.id;
                    if artist_id == 0 {
                        return Ok(());
                    }
                    let name = format!("{}: {}", column.header, singer.name);
                    self.state.navigation.push_breadcrumb();
                    self.state.navigation.set_content(ContentState::Loading);
                    let generation = self.bump_content_generation();
                    let service = self.service.clone();
                    let sender = self.events.sender();
                    let limit = self.config.search_limit;
                    tokio::spawn(async move {
                        let state = service.load_artist_songs(artist_id, limit).await;
                        let _ = sender.send(
                            NavigationEvent::ContentLoaded {
                                content: state,
                                generation,
                            }
                            .into(),
                        );
                        let _ =
                            sender.send(NavigationEvent::BreadcrumbSet { name, generation }.into());
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_upload_cached_song(&mut self, row: usize) {
        let songs = match self.state.navigation.content.as_ref() {
            ContentState::Songs(songs) => songs,
            _ => return,
        };
        let song = match songs.get(row) {
            Some(s) => s.clone(),
            None => return,
        };

        self.toast(format!(
            "{} 正在上传 {}...",
            crate::icons::UPLOAD,
            song.name
        ));

        let is_local = song.copyright == ncm_api::SongCopyright::Free
            && !song.album.is_empty()
            && std::path::Path::new(&song.album).exists();
        let service = self.service.clone();
        let cache = self.service.cache().clone();
        let sender = self.events.sender();
        let song_id = song.id;
        let cached_path: Option<PathBuf> = if is_local {
            Some(std::path::PathBuf::from(&song.album))
        } else {
            const EXTS: &[&str] = &["mp3", "flac", "m4a", "ogg"];
            EXTS.iter().find_map(|ext| {
                let p = cache.cache_path(song_id, ext);
                if p.exists() { Some(p) } else { None }
            })
        };

        tokio::spawn(async move {
            let path = match cached_path {
                Some(p) => p,
                None => {
                    send_event(&sender, AppEvent::Toast("未找到文件".into()).into());
                    return;
                }
            };

            match service
                .upload_song_with_meta(&path, &song.name, &song.album, &song.singer)
                .await
            {
                Ok(result) => {
                    cache.mark_uploaded(song_id);
                    log::info!("Uploaded {} (song_id={})", result.song_name, result.song_id);
                    send_event(
                        &sender,
                        NavigationEvent::NavSelect(ApiEndpoint::Download.as_str().into()).into(),
                    );
                    send_event(
                        &sender,
                        AppEvent::Toast(format!(
                            "{} 上传成功: {}",
                            crate::icons::UPLOAD,
                            result.song_name
                        ))
                        .into(),
                    );
                }
                Err(e) => {
                    log::error!("Upload failed for song_id={song_id}: {e}");
                    send_event(
                        &sender,
                        AppEvent::Toast(format!("{} 上传失败: {e}", crate::icons::UPLOAD)).into(),
                    );
                }
            }
        });
    }
}
