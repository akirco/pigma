//! Cover-byte infrastructure: URL sizing, downloads, and the on-disk cover
//! cache. Turning those bytes into terminal protocols is a presentation
//! concern and lives in [`crate::ui::cover`].

mod disk;

pub(crate) async fn fetch_cover(http: &reqwest::Client, url: &str, px: u32) -> Option<Vec<u8>> {
    let url = sized_cover_url(url, px);
    let response = http.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.bytes().await.ok().map(|bytes| bytes.to_vec())
}

/// Ask the cover CDN for a small image where supported.
///
/// NetEase/Kuwo accept `?param=NxN`, but Bilibili ignores it and uses an
/// `@Nw_Nh_1c.jpg` path suffix instead (`1c` = crop). Keep `.jpg`: the `image`
/// crate is built with the JPEG decoder only.
pub(crate) fn sized_cover_url(url: &str, px: u32) -> String {
    let is_bilibili = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| host == "hdslb.com" || host.ends_with(".hdslb.com"));
    if is_bilibili {
        let (base, query) = url
            .split_once('?')
            .map_or((url, None), |(base, query)| (base, Some(query)));
        // Drop an existing `@...` size suffix so repeated calls stay stable.
        let base = base.split('@').next().unwrap_or(base);
        let sized = format!("{base}@{px}w_{px}h_1c.jpg");
        return match query {
            Some(query) => format!("{sized}?{query}"),
            None => sized,
        };
    }

    if url.contains('?') {
        format!("{url}&param={px}y{px}")
    } else {
        format!("{url}?param={px}y{px}")
    }
}

#[cfg(test)]
mod tests {
    use super::sized_cover_url;

    #[test]
    fn bilibili_covers_use_the_path_suffix() {
        let base = "https://i1.hdslb.com/bfs/archive/abc.jpg";
        assert_eq!(
            sized_cover_url(base, 200),
            "https://i1.hdslb.com/bfs/archive/abc.jpg@200w_200h_1c.jpg"
        );
        assert_eq!(
            sized_cover_url(
                "https://i1.hdslb.com/bfs/archive/abc.jpg@480w_270h_1c.webp",
                200
            ),
            "https://i1.hdslb.com/bfs/archive/abc.jpg@200w_200h_1c.jpg"
        );
        assert_eq!(
            sized_cover_url("https://i1.hdslb.com/bfs/archive/abc.jpg?x=1", 200),
            "https://i1.hdslb.com/bfs/archive/abc.jpg@200w_200h_1c.jpg?x=1"
        );
    }

    #[test]
    fn netease_covers_keep_the_param_query() {
        assert_eq!(
            sized_cover_url("https://p1.music.126.net/x.jpg", 200),
            "https://p1.music.126.net/x.jpg?param=200y200"
        );
        assert_eq!(
            sized_cover_url("https://p1.music.126.net/x.jpg?a=1", 200),
            "https://p1.music.126.net/x.jpg?a=1&param=200y200"
        );
    }
}
