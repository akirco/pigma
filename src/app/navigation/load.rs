//! Navigation target loading: endpoint resolution, content-cache reads/writes
//! and spawning the background fetch that reports back via events.

use super::super::{App, content::content_cache_key, event::send_event};
use crate::{
    event::{NavigationEvent, PlaybackEvent},
    service::ApiEndpoint,
    state::{ContentState, Page},
};

impl App {
    /// Reload the content for the current navigation item.
    ///
    /// With `force = true`, skip the content cache, refetch directly from the API,
    /// and write the result back to the cache (manual refresh).
    pub(crate) fn handle_nav_select(&mut self, api_str: String, force: bool) -> eyre::Result<()> {
        if api_str == "login" {
            self.state.navigation.page = Page::Login;
            return Ok(());
        }
        let api = match ApiEndpoint::parse(&api_str) {
            Some(ep) => ep,
            None => {
                self.state
                    .navigation
                    .set_content(ContentState::Error(format!("未知: {api_str}")));
                return Ok(());
            }
        };

        // A new navigation target invalidates every in-flight content load.
        let generation = self.bump_content_generation();

        if api == ApiEndpoint::LocalMusic {
            self.state.navigation.content_is_search = false;
            self.state.navigation.clear_breadcrumb();
            self.state.navigation.set_content(ContentState::Loading);
            let service = self.service.clone();
            let content_cache = self.service.content_cache().clone();
            let ttl = self.config.cache.content_cache_ttl;
            let sender = self.events.sender();

            tokio::spawn(async move {
                if ttl > 0
                    && let Some((cached, _)) =
                        content_cache.load_content_cache_async(&api_str, ttl).await
                {
                    send_event(
                        &sender,
                        NavigationEvent::ContentLoaded {
                            content: cached,
                            generation,
                        }
                        .into(),
                    );
                    return;
                }
                let state = service.load_local_music().await;
                let state = if ttl > 0 {
                    let cache_clone = content_cache.clone();
                    tokio::task::spawn_blocking(move || {
                        cache_clone.save_content_cache(
                            ApiEndpoint::LocalMusic.as_str(),
                            &state,
                            None,
                        );
                        state
                    })
                    .await
                    .unwrap_or(ContentState::Empty)
                } else {
                    state
                };
                send_event(
                    &sender,
                    NavigationEvent::ContentLoaded {
                        content: state,
                        generation,
                    }
                    .into(),
                );
            });
            return Ok(());
        }

        self.state.navigation.clear_breadcrumb();
        self.state.navigation.content_is_search = false;
        self.state.navigation.set_content(ContentState::Loading);
        self.state.navigation.nav.subtitle = None;
        if api == ApiEndpoint::Search {
            self.state.navigation.nav.subtitle = Some("热搜榜".into());
        }
        let cache = self.service.cache().clone();
        let content_cache = self.service.content_cache().clone();
        let service = self.service.clone();
        let sender = self.events.sender();
        let uid = self.state.navigation.user.as_ref().map(|u| u.uid);
        let ttl = self.config.cache.content_cache_ttl;
        let limit = self.config.search_limit;

        tokio::spawn(async move {
            if api == ApiEndpoint::Download {
                let songs = cache.list_cached_songs_async().await;
                send_event(
                    &sender,
                    NavigationEvent::ContentLoaded {
                        content: ContentState::Songs(
                            songs.into_iter().map(std::sync::Arc::new).collect(),
                        ),
                        generation,
                    }
                    .into(),
                );
                return;
            }

            if ttl > 0
                && !force
                && !matches!(api, ApiEndpoint::Search | ApiEndpoint::LikedSongs)
                && let Some(cache_key) = content_cache_key(&api_str, uid)
                && let Some((cached, pg)) = content_cache
                    .load_content_cache_async(&cache_key, ttl)
                    .await
            {
                // When restoring playlist pagination from cache, top up the trackIds, otherwise lazy pagination (LoadMore) cannot slice.
                if let Some(pg) = &pg
                    && let Some(id_str) = pg.api.strip_prefix("playlist:")
                    && let Ok(id) = id_str.parse::<u64>()
                {
                    let service = service.clone();
                    tokio::spawn(async move {
                        if let Some(ids) = service.ensure_playlist_track_ids(id).await {
                            log::debug!("refetched trackIds for playlist {id}: {} ids", ids);
                        }
                    });
                }

                if let Some(pg) = pg {
                    send_event(
                        &sender,
                        NavigationEvent::ContentLoadedPaged {
                            content: cached,
                            pagination: pg,
                            generation,
                        }
                        .into(),
                    );
                } else {
                    send_event(
                        &sender,
                        NavigationEvent::ContentLoaded {
                            content: cached,
                            generation,
                        }
                        .into(),
                    );
                }
                return;
            }

            // Handle LikedSongs separately: also fetch playlist ID for heartbeat mode.
            // Paginate lazily through the "我喜欢的音乐" playlist's trackIds to avoid stalling from fetching everything at once.
            if api == ApiEndpoint::LikedSongs
                && let Some(uid) = uid
            {
                let (state, pagination, playlist_id) = service.load_liked_songs(uid, limit).await;
                if let Some(pg) = pagination {
                    send_event(
                        &sender,
                        NavigationEvent::ContentLoadedPaged {
                            content: state,
                            pagination: pg,
                            generation,
                        }
                        .into(),
                    );
                } else {
                    send_event(
                        &sender,
                        NavigationEvent::ContentLoaded {
                            content: state,
                            generation,
                        }
                        .into(),
                    );
                }
                if let Some(id) = playlist_id {
                    send_event(&sender, PlaybackEvent::SetPlaylistId(id).into());
                }
                return;
            }

            let (state, pagination) = service.resolve_content(api, uid, limit).await;

            let state = if ttl > 0
                && api != ApiEndpoint::Search
                && !matches!(state, ContentState::Error(_))
                && let Some(cache_key) = content_cache_key(&api_str, uid)
            {
                let cache_clone = content_cache.clone();
                let pg_for_save = pagination.clone();
                tokio::task::spawn_blocking(move || {
                    cache_clone.save_content_cache(&cache_key, &state, pg_for_save.as_ref());
                    state
                })
                .await
                .unwrap_or(ContentState::Empty)
            } else {
                state
            };

            if let Some(pg) = pagination {
                send_event(
                    &sender,
                    NavigationEvent::ContentLoadedPaged {
                        content: state,
                        pagination: pg,
                        generation,
                    }
                    .into(),
                );
            } else {
                send_event(
                    &sender,
                    NavigationEvent::ContentLoaded {
                        content: state,
                        generation,
                    }
                    .into(),
                );
            }
        });
        Ok(())
    }
}
