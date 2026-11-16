//! Content load lifecycle: generation guards, page merges and lazy pagination.

use super::{
    super::{App, event::send_event},
    key::content_cache_key,
};
use crate::{
    event::NavigationEvent,
    state::{ContentState, PaginationInfo},
};

impl App {
    /// Start a new content load generation. Every async content/search load
    /// captures this value and carries it on its result event, so responses
    /// that arrive after the user switched pages can be dropped instead of
    /// overwriting the current view.
    pub(crate) fn bump_content_generation(&mut self) -> u64 {
        let generation = self.state.navigation.generation.wrapping_add(1);
        self.state.navigation.generation = generation;
        generation
    }

    /// A dropped response must not leave pagination permanently in flight:
    /// `check_load_more` refuses to start another page while `loading` is set.
    fn release_pagination_loading(&mut self) {
        if let Some(pg) = self.state.navigation.pagination.as_mut() {
            pg.loading = false;
        }
    }

    pub(crate) fn handle_content_loaded(&mut self, content: ContentState, generation: u64) {
        if generation != self.state.navigation.generation {
            self.release_pagination_loading();
            return;
        }
        self.state.navigation.set_content(content);
    }

    pub(crate) fn handle_load_more(&mut self) {
        let (api, offset, limit) = match self.state.navigation.pagination.as_ref() {
            Some(pg) if pg.has_more && !pg.loading => (pg.api.clone(), pg.next_offset(), pg.limit),
            _ => return,
        };

        let service = self.service.clone();
        let sender = self.events.sender();
        let generation = self.state.navigation.generation;

        tokio::spawn(async move {
            match service.load_more(&api, offset, limit).await {
                Some((content, pagination)) => send_event(
                    &sender,
                    NavigationEvent::ContentLoadedPaged {
                        content,
                        pagination,
                        generation,
                    }
                    .into(),
                ),
                // Release the in-flight flag, otherwise pagination stays stuck
                // after a single transient failure.
                None => send_event(&sender, NavigationEvent::LoadMoreFailed.into()),
            }
        });
    }

    pub(crate) fn handle_content_loaded_paged(
        &mut self,
        content: ContentState,
        pagination: PaginationInfo,
        generation: u64,
    ) {
        // Drop stale responses, but release the in-flight flag first.
        if generation != self.state.navigation.generation {
            self.release_pagination_loading();
            return;
        }

        let same_api =
            self.state.navigation.pagination.as_ref().map(|p| &p.api) == Some(&pagination.api);

        let mut content = content;

        // Only song lists (cloud disk, songs within a playlist) support paged appends; other types replace the whole content.
        if same_api
            && let ContentState::Songs(new_songs) = &mut content
            && matches!(
                self.state.navigation.content.as_ref(),
                ContentState::Songs(_)
            )
        {
            std::sync::Arc::make_mut(&mut self.state.navigation.content)
                .append_unique_songs(std::mem::take(new_songs));
            let pg_for_save = pagination.clone();
            self.state.navigation.pagination = Some(pagination);

            let ttl = self.config.cache.content_cache_ttl;
            let cache_key = content_cache_key(
                &pg_for_save.api,
                self.state.navigation.user.as_ref().map(|u| u.uid),
            );
            if ttl > 0
                && !pg_for_save.api.is_empty()
                && let Some(cache_key) = cache_key
            {
                let cache = self.service.content_cache().clone();
                // Hand the blocking writer a *clone* of the merged content. For
                // `Songs` this is just a cheap `Vec<Arc<_>>` copy — no deep
                // clone, and no shared `Arc` outlives this function, so the next
                // `Arc::make_mut` is not forced into a deep clone. Serialization
                // and the disk write then happen off-thread, and a burst of
                // pages coalesces into one write of the final state
                // (`queue_content_cache`).
                let merged = self.state.navigation.content.as_ref().clone();
                cache.queue_content_cache(&cache_key, merged, pg_for_save);
            }
            return;
        }
        self.state.navigation.set_content(content);
        self.state.navigation.pagination = Some(pagination);
    }
}
