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
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "recommend_songs" => Some(ApiEndpoint::RecommendSongs),
            "recommend_resource" => Some(ApiEndpoint::RecommendResource),
            "toplist" => Some(ApiEndpoint::Toplist),
            "top_song_list" => Some(ApiEndpoint::TopSongList),
            "user_radio_sublist" => Some(ApiEndpoint::UserRadioSublist),
            "user_cloud_disk" => Some(ApiEndpoint::UserCloudDisk),
            "__liked__" => Some(ApiEndpoint::LikedSongs),
            "user_song_list" => Some(ApiEndpoint::UserSongList),
            "user_created_song_list" => Some(ApiEndpoint::UserCreatedSongList),
            "user_subscribed_song_list" => Some(ApiEndpoint::UserSubscribedSongList),
            "album_sublist" => Some(ApiEndpoint::SavedAlbums),
            "__download__" => Some(ApiEndpoint::Download),
            "__local_music__" => Some(ApiEndpoint::LocalMusic),
            "__recent__" => Some(ApiEndpoint::Recent),
            "search" => Some(ApiEndpoint::Search),
            "top_singers" => Some(ApiEndpoint::TopSingers),
            _ => None,
        }
    }
}
