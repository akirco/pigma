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
    Download,
    LocalMusic,
    Recent,
    Search,
    TopSingers,
}

impl ApiEndpoint {
    pub const ALL: &'static [ApiEndpoint] = &[
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
        ApiEndpoint::Download,
        ApiEndpoint::LocalMusic,
        ApiEndpoint::Recent,
        ApiEndpoint::Search,
        ApiEndpoint::TopSingers,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ApiEndpoint::RecommendSongs => "recommend_songs",
            ApiEndpoint::RecommendResource => "recommend_resource",
            ApiEndpoint::Toplist => "toplist",
            ApiEndpoint::TopSongList => "top_song_list",
            ApiEndpoint::UserRadioSublist => "user_radio_sublist",
            ApiEndpoint::UserCloudDisk => "user_cloud_disk",
            ApiEndpoint::LikedSongs => "__liked__",
            ApiEndpoint::UserSongList => "user_song_list",
            ApiEndpoint::UserCreatedSongList => "user_created_song_list",
            ApiEndpoint::UserSubscribedSongList => "user_subscribed_song_list",
            ApiEndpoint::Download => "__download__",
            ApiEndpoint::LocalMusic => "__local_music__",
            ApiEndpoint::Recent => "__recent__",
            ApiEndpoint::Search => "search",
            ApiEndpoint::TopSingers => "top_singers",
        }
    }

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
            "__download__" => Some(ApiEndpoint::Download),
            "__local_music__" => Some(ApiEndpoint::LocalMusic),
            "__recent__" => Some(ApiEndpoint::Recent),
            "search" => Some(ApiEndpoint::Search),
            "top_singers" => Some(ApiEndpoint::TopSingers),
            _ => None,
        }
    }

    pub fn needs_login(&self) -> bool {
        matches!(
            self,
            ApiEndpoint::LikedSongs
                | ApiEndpoint::UserSongList
                | ApiEndpoint::UserCreatedSongList
                | ApiEndpoint::UserSubscribedSongList
        )
    }
}
