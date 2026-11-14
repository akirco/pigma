//! Cache-key derivation for endpoint content. Personal endpoints are scoped by
//! the signed-in uid so accounts never see each other's cached pages.

use crate::service::ApiEndpoint;

/// Cache key for an endpoint's content.
///
/// Personal endpoints are scoped by the signed-in uid, so logging into another
/// account cannot show the previous account's recent/playlists/cloud disk from
/// the cache. Public endpoints keep their plain name. Returns `None` when a
/// personal endpoint is requested without a uid (nothing should be cached or
/// served from cache in that state).
pub(crate) fn content_cache_key(api: &str, uid: Option<u64>) -> Option<String> {
    match ApiEndpoint::parse(api) {
        Some(endpoint) if endpoint.is_personal() => uid.map(|uid| format!("u{uid}-{api}")),
        _ => Some(api.to_string()),
    }
}
#[cfg(test)]
mod tests {
    use super::content_cache_key;

    #[test]
    fn personal_content_cache_is_scoped_by_uid() {
        assert_eq!(
            content_cache_key("recent", Some(42)).as_deref(),
            Some("u42-recent")
        );
        assert_eq!(
            content_cache_key("user_cloud_disk", Some(42)).as_deref(),
            Some("u42-user_cloud_disk")
        );
        assert_eq!(content_cache_key("recent", None), None);
    }

    #[test]
    fn public_and_non_endpoint_keys_are_unchanged() {
        assert_eq!(
            content_cache_key("toplist", Some(42)).as_deref(),
            Some("toplist")
        );
        assert_eq!(
            content_cache_key("local_music", None).as_deref(),
            Some("local_music")
        );
        assert_eq!(
            content_cache_key("playlist:7", None).as_deref(),
            Some("playlist:7")
        );
    }
}
