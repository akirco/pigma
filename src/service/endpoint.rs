/// Navigation/content endpoints backed by the NetEase Cloud Music API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiEndpoint {
    RecommendSongs,
    RecommendResource,
    Toplist,
    TopSongList,
    UserRadioSublist,
    UserCloudDisk,
    LikedSongs,
    UserSongList,
    UserCreatedSongList,
    UserSubscribedSongList,
    SavedAlbums,
    Download,
    LocalMusic,
    Recent,
    Search,
    TopSingers,
}

impl ApiEndpoint {
    /// Canonical string key used by the nav config and the content cache.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RecommendSongs => "recommend_songs",
            Self::RecommendResource => "recommend_resource",
            Self::Toplist => "toplist",
            Self::TopSongList => "top_song_list",
            Self::UserRadioSublist => "user_radio_sublist",
            Self::UserCloudDisk => "user_cloud_disk",
            Self::LikedSongs => "liked",
            Self::UserSongList => "user_song_list",
            Self::UserCreatedSongList => "user_created_song_list",
            Self::UserSubscribedSongList => "user_subscribed_song_list",
            Self::SavedAlbums => "album_sublist",
            Self::Download => "download",
            Self::LocalMusic => "local_music",
            Self::Recent => "recent",
            Self::Search => "search",
            Self::TopSingers => "top_singers",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "recommend_songs" => Self::RecommendSongs,
            "recommend_resource" => Self::RecommendResource,
            "toplist" => Self::Toplist,
            "top_song_list" => Self::TopSongList,
            "user_radio_sublist" => Self::UserRadioSublist,
            "user_cloud_disk" => Self::UserCloudDisk,
            "liked" | "__liked__" => Self::LikedSongs,
            "user_song_list" => Self::UserSongList,
            "user_created_song_list" => Self::UserCreatedSongList,
            "user_subscribed_song_list" => Self::UserSubscribedSongList,
            "album_sublist" => Self::SavedAlbums,
            "download" | "__download__" => Self::Download,
            "local_music" | "__local_music__" => Self::LocalMusic,
            "recent" | "__recent__" => Self::Recent,
            "search" => Self::Search,
            "top_singers" => Self::TopSingers,
            _ => return None,
        })
    }

    /// Whether the endpoint returns data scoped to the signed-in account.
    ///
    /// Used both to gate the request (login required) and to scope the content
    /// cache so switching accounts cannot show another account's lists.
    pub const fn is_personal(self) -> bool {
        matches!(
            self,
            Self::RecommendSongs
                | Self::RecommendResource
                | Self::UserRadioSublist
                | Self::UserCloudDisk
                | Self::LikedSongs
                | Self::UserSongList
                | Self::UserCreatedSongList
                | Self::UserSubscribedSongList
                | Self::SavedAlbums
                | Self::Recent
        )
    }
}

#[cfg(test)]
mod tests {
    use super::ApiEndpoint;

    const ALL: &[ApiEndpoint] = &[
        ApiEndpoint::RecommendSongs,
        ApiEndpoint::RecommendResource,
        ApiEndpoint::Toplist,
        ApiEndpoint::TopSongList,
        ApiEndpoint::UserRadioSublist,
        ApiEndpoint::UserCloudDisk,
        ApiEndpoint::LikedSongs,
        ApiEndpoint::UserSongList,
        ApiEndpoint::UserCreatedSongList,
        ApiEndpoint::UserSubscribedSongList,
        ApiEndpoint::SavedAlbums,
        ApiEndpoint::Download,
        ApiEndpoint::LocalMusic,
        ApiEndpoint::Recent,
        ApiEndpoint::Search,
        ApiEndpoint::TopSingers,
    ];

    #[test]
    fn parse_round_trips_the_canonical_key() {
        for endpoint in ALL {
            assert_eq!(ApiEndpoint::parse(endpoint.as_str()), Some(*endpoint));
        }
    }

    #[test]
    fn parse_accepts_the_legacy_aliases() {
        assert_eq!(
            ApiEndpoint::parse("__liked__"),
            Some(ApiEndpoint::LikedSongs)
        );
        assert_eq!(
            ApiEndpoint::parse("__download__"),
            Some(ApiEndpoint::Download)
        );
        assert_eq!(
            ApiEndpoint::parse("__local_music__"),
            Some(ApiEndpoint::LocalMusic)
        );
        assert_eq!(ApiEndpoint::parse("__recent__"), Some(ApiEndpoint::Recent));
        assert_eq!(ApiEndpoint::parse("nope"), None);
    }

    #[test]
    fn personal_endpoints_are_the_login_gated_ones() {
        assert!(ApiEndpoint::RecommendSongs.is_personal());
        assert!(ApiEndpoint::Recent.is_personal());
        assert!(ApiEndpoint::LikedSongs.is_personal());
        assert!(ApiEndpoint::UserCloudDisk.is_personal());
        assert!(!ApiEndpoint::Toplist.is_personal());
        assert!(!ApiEndpoint::Download.is_personal());
        assert!(!ApiEndpoint::LocalMusic.is_personal());
        assert!(!ApiEndpoint::Search.is_personal());
    }
}
