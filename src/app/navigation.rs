use std::future::Future;
use std::sync::Arc;

use super::{App, send_event};
use crate::api::ApiEndpoint;
use crate::event::{AppEvent, Event};
use crate::state::{ContentState, PaginationInfo};

impl App {
    pub(super) fn handle_nav_select(&mut self, api_str: String) -> color_eyre::Result<()> {
        let api = match ApiEndpoint::parse(&api_str) {
            Some(ep) => ep,
            None => {
                self.state
                    .navigation
                    .set_content(ContentState::Error(format!("未知: {api_str}")));
                return Ok(());
            }
        };

        if api == ApiEndpoint::LocalMusic {
            self.state
                .navigation
                .set_content(self.state.local_music.clone());
            self.state.navigation.content_selected = 0;
            return Ok(());
        }
        if api == ApiEndpoint::Download {
            let cache = self.playback.cache().clone();
            let songs = cache.list_cached_songs();
            self.state
                .navigation
                .set_content(ContentState::Songs(songs));
            self.state.navigation.content_selected = 0;
            return Ok(());
        }
        let ttl = self.config.content_cache_ttl;
        if ttl > 0
            && api != ApiEndpoint::Search
            && let Some(cached) = self.playback.cache().load_content_cache(&api_str, ttl)
        {
            self.state.navigation.set_content(cached);
            self.state.navigation.content_selected = 0;
            return Ok(());
        }

        self.state.navigation.clear_breadcrumb();
        self.state.navigation.set_content(ContentState::Loading);
        self.state.navigation.nav.subtitle = None;
        if api == ApiEndpoint::Search {
            self.state.navigation.nav.subtitle = Some("热搜榜".into());
        }
        let cache = self.playback.cache().clone();
        let api_client = self.api.clone();
        let sender = self.state.events.sender();
        let uid = self.state.navigation.user.as_ref().map(|u| u.uid);

        tokio::spawn(async move {
            // Handle LikedSongs separately: also fetch playlist ID for heartbeat mode
            if api == ApiEndpoint::LikedSongs
                && let Some(uid) = uid
            {
                let songs_result = api_client.liked_songs(uid).await;
                match songs_result {
                    Ok(songs) => {
                        let state = ContentState::Songs(songs);
                        send_event(&sender, Event::App(AppEvent::ContentLoaded(state)));
                        if let Ok(lists) = api_client.user_song_list(uid, 0, 50).await
                            && let Some(liked) = lists.iter().find(|l| l.name == "我喜欢的音乐")
                        {
                            send_event(&sender, Event::App(AppEvent::SetPlaylistId(liked.id)));
                        }
                    }
                    Err(e) => {
                        send_event(
                            &sender,
                            Event::App(AppEvent::ContentLoaded(ContentState::Error(e.to_string()))),
                        );
                    }
                }
                return;
            }

            let result = match api {
                ApiEndpoint::RecommendResource => api_client
                    .recommend_resource()
                    .await
                    .map(|c| (ContentState::SongLists(c), None)),
                ApiEndpoint::Toplist => api_client
                    .toplist()
                    .await
                    .map(|c| (ContentState::TopLists(c), None)),
                ApiEndpoint::TopSongList => api_client
                    .top_song_list("全部", "hot", 0, 50)
                    .await
                    .map(|c| (ContentState::SongLists(c), None)),
                ApiEndpoint::UserRadioSublist => api_client
                    .user_radio_sublist(0, 50)
                    .await
                    .map(|c| (ContentState::SongLists(c), None)),
                ApiEndpoint::RecommendSongs => api_client
                    .recommend_songs()
                    .await
                    .map(|c| (ContentState::Songs(c), None)),
                ApiEndpoint::UserCloudDisk => {
                    let api_str_clone = api_str.clone();
                    api_client.user_cloud_disk(0, 50).await.map(move |result| {
                        (
                            ContentState::Songs(result.songs),
                            Some(PaginationInfo {
                                api: api_str_clone,
                                offset: 0,
                                limit: 50,
                                has_more: result.has_more,
                                total: result.count,
                                loading: false,
                            }),
                        )
                    })
                }
                ApiEndpoint::Recent => api_client
                    .recent_songs(100)
                    .await
                    .map(|c| (ContentState::Songs(c), None)),
                ApiEndpoint::UserSongList => {
                    if let Some(uid) = uid {
                        api_client
                            .user_song_list(uid, 0, 50)
                            .await
                            .map(|c| (ContentState::SongLists(c), None))
                    } else {
                        Ok((ContentState::Error("未登录".into()), None))
                    }
                }
                ApiEndpoint::UserCreatedSongList => {
                    if let Some(uid) = uid {
                        api_client
                            .user_created_playlist(uid, 0, 50)
                            .await
                            .map(|c| (ContentState::SongLists(c), None))
                    } else {
                        Ok((ContentState::Error("未登录".into()), None))
                    }
                }
                ApiEndpoint::UserSubscribedSongList => {
                    if let Some(uid) = uid {
                        api_client
                            .user_collected_playlist(uid, 0, 50)
                            .await
                            .map(|c| (ContentState::SongLists(c), None))
                    } else {
                        Ok((ContentState::Error("未登录".into()), None))
                    }
                }
                ApiEndpoint::Search => api_client.search_hot().await.map(|items| {
                    (
                        ContentState::HotSearch(items.iter().map(|h| h.keyword.clone()).collect()),
                        None,
                    )
                }),
                ApiEndpoint::Download => unreachable!(),
                ApiEndpoint::LocalMusic => unreachable!(),
                ApiEndpoint::TopSingers => api_client
                    .top_artists(0, 50)
                    .await
                    .map(|c| (ContentState::Singers(c), None)),
                ApiEndpoint::LikedSongs => unreachable!(),
            };

            let (state, pagination) = match result {
                Ok((content, pg)) => (content, pg),
                Err(e) => (ContentState::Error(e.to_string()), None),
            };

            if ttl > 0 && api != ApiEndpoint::Search {
                let cache_clone = cache.clone();
                let api_str_clone = api_str.clone();
                let state_clone = state.clone();
                tokio::task::spawn_blocking(move || {
                    cache_clone.save_content_cache(&api_str_clone, state_clone);
                })
                .await
                .ok();
            }

            if let Some(pg) = pagination {
                send_event(
                    &sender,
                    Event::App(AppEvent::ContentLoadedPaged {
                        content: state,
                        pagination: pg,
                    }),
                );
            } else {
                send_event(&sender, Event::App(AppEvent::ContentLoaded(state)));
            }
        });
        Ok(())
    }

    pub(super) fn handle_breadcrumb(&mut self, name: String) {
        self.state.navigation.nav.subtitle = Some(name);
    }

    pub(super) fn navigate_to_entity<F, Fut>(&mut self, name: String, api_call: F)
    where
        F: FnOnce(Arc<ncm_api::NcmClient>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Vec<ncm_api::SongInfo>, ncm_api::NcmError>> + Send,
    {
        self.state.navigation.push_breadcrumb();
        self.state.navigation.set_content(ContentState::Loading);

        let api = self.api.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            let result = api_call(api).await;
            let state = match result {
                Ok(songs) => ContentState::Songs(songs),
                Err(e) => ContentState::Error(e.to_string()),
            };
            let _ = sender.send(Event::App(AppEvent::ContentLoaded(state)));
            let _ = sender.send(Event::App(AppEvent::BreadcrumbSet(name)));
        });
    }

    pub(super) fn handle_cell_action(&mut self, row: usize, col: usize) -> color_eyre::Result<()> {
        let columns = self
            .config
            .columns
            .for_content(self.state.navigation.content.content_type(), None)
            .to_vec();
        let column = match columns.get(col) {
            Some(c) => c.clone(),
            None => return Ok(()),
        };
        let field = column.field.clone();

        match (self.state.navigation.content.as_ref(), field.as_str()) {
            (ContentState::Songs(songs), "album") => {
                if let Some(song) = songs.get(row) {
                    let album_id = song.album_id;
                    let name = format!("{}: {}", column.header, song.album);
                    self.navigate_to_entity(name, move |api| async move {
                        api.album(album_id).await.map(|d| d.songs)
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
                    self.navigate_to_entity(name, move |api| async move {
                        api.singer_songs(artist_id).await
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
                    self.navigate_to_entity(name, move |api| async move {
                        api.singer_songs(artist_id).await
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }
}
