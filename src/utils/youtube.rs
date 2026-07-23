use ncm_api::SongInfo;

/// Convert common Traditional Chinese characters to Simplified.
/// Covers artist names, song titles, and common music-related words.
fn normalize_cjk(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            // Surnames (most common in Chinese music)
            '張' => '张',
            '劉' => '刘',
            '陳' => '陈',
            '楊' => '杨',
            '黃' => '黄',
            '趙' => '赵',
            '吳' => '吴',
            '孫' => '孙',
            '馬' => '马',
            '羅' => '罗',
            '鄭' => '郑',
            '謝' => '谢',
            '許' => '许',
            '韓' => '韩',
            '馮' => '邓',
            '鄧' => '邓',
            '蕭' => '萧',
            '蔡' => '蔡',
            '蔣' => '蒋',
            '葉' => '叶',
            '蘇' => '苏',
            '盧' => '卢',
            '鍾' => '钟',
            '陸' => '陆',
            '範' => '范',
            '韋' => '韦',
            '賈' => '贾',
            '鄒' => '邹',
            '閆' => '闫',
            '龐' => '庞',
            '龔' => '龚',
            '歐' => '欧',
            '顧' => '顾',
            '嚴' => '严',
            '萬' => '万',
            '龍' => '龙',
            // Common given name characters
            '傑' => '杰',
            '倫' => '伦',
            '偉' => '伟',
            '強' => '强',
            '華' => '华',
            '國' => '国',
            '東' => '东',
            '軍' => '军',
            '恆' => '恒',
            '澤' => '泽',
            '凱' => '凯',
            '鳳' => '凤',
            '飛' => '飞',
            '鑫' => '鑫',
            '磊' => '磊',
            '毅' => '毅',
            // Song title / music words
            '樂' => '乐',
            '風' => '风',
            '雲' => '云',
            '愛' => '爱',
            '夢' => '梦',
            '鄉' => '乡',
            '園' => '园',
            '樹' => '树',
            '鳥' => '鸟',
            '魚' => '鱼',
            '書' => '书',
            '畫' => '画',
            '詩' => '诗',
            '詞' => '词',
            '調' => '调',
            '聲' => '声',
            '頭' => '头',
            '淚' => '泪',
            '離' => '离',
            '別' => '别',
            '緣' => '缘',
            '憶' => '忆',
            '諾' => '诺',
            '約' => '约',
            // Common verbs / function words in titles
            '說' => '说',
            '問' => '问',
            '聽' => '听',
            '見' => '见',
            '讀' => '读',
            '寫' => '写',
            '記' => '记',
            '認' => '认',
            '請' => '请',
            '讓' => '让',
            '對' => '对',
            '從' => '从',
            '過' => '过',
            '還' => '还',
            '進' => '进',
            '開' => '开',
            '關' => '关',
            '買' => '买',
            '賣' => '卖',
            '長' => '长',
            '單' => '单',
            '雙' => '双',
            '個' => '个',
            '無' => '无',
            '來' => '来',
            '時' => '时',
            '間' => '间',
            '歲' => '岁',
            '點' => '点',
            '幾' => '几',
            '億' => '亿',
            '數' => '数',
            // Nature / scenery / objects
            '處' => '处',
            '門' => '门',
            '橋' => '桥',
            '車' => '车',
            '機' => '机',
            '電' => '电',
            '話' => '话',
            '視' => '视',
            '網' => '网',
            '節' => '节',
            '歷' => '历',
            '實' => '实',
            '據' => '据',
            '構' => '构',
            '選' => '选',
            '遲' => '迟',
            '連' => '连',
            '達' => '达',
            '運' => '运',
            '遠' => '远',
            '邊' => '边',
            '錯' => '错',
            '聞' => '闻',
            '難' => '难',
            _ => c,
        })
        .collect()
}

/// Normalize a string for comparison: lowercase + CJK conversion + trim.
fn normalize_for_match(s: &str) -> String {
    normalize_cjk(&s.to_lowercase()).trim().to_string()
}

/// Parse a YouTube views string like "1,234,567 views" into a number.
pub fn parse_view_count(s: &str) -> u64 {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    cleaned.parse().unwrap_or(0)
}

/// Parse a duration string like "3:33" or "1:02:33" into seconds.
pub fn parse_duration_str(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        2 => {
            let min: u64 = parts[0].parse().ok()?;
            let sec: u64 = parts[1].parse().ok()?;
            Some(min * 60 + sec)
        }
        3 => {
            let hr: u64 = parts[0].parse().ok()?;
            let min: u64 = parts[1].parse().ok()?;
            let sec: u64 = parts[2].parse().ok()?;
            Some(hr * 3600 + min * 60 + sec)
        }
        _ => None,
    }
}

/// Strip annotations like "(Live)", "[歌词版]", "(DJ版)" from a song name.
pub fn strip_annotation(s: &str) -> String {
    s.split(['(', '（', '[', '【'])
        .next()
        .map(|v| v.trim().to_string())
        .unwrap_or_default()
}

/// Clean a song name for YouTube search by stripping common annotations.
pub fn clean_search_query(name: &str) -> String {
    strip_annotation(name)
}

/// Score name match: 0-30 points.
/// Position-aware: exact=30, prefix=25, suffix=22, in-brackets=20, substring by length.
fn score_name(title: &str, name: &str) -> u32 {
    if name.is_empty() {
        return 0;
    }

    // Exact match after normalization
    if title == name {
        return 30;
    }

    // Name is a prefix of the title
    if title.starts_with(name) {
        return 25;
    }

    // Name is a suffix of the title
    if title.ends_with(name) {
        return 22;
    }

    // Name appears inside brackets/parentheses
    if let Some(pos) = title.find(name) {
        let before = &title[..pos];
        let after = &title[pos + name.len()..];
        let in_brackets = (before.ends_with('【')
            || before.ends_with('[')
            || before.ends_with('(')
            || before.ends_with('（'))
            && (after.starts_with('】')
                || after.starts_with(']')
                || after.starts_with(')')
                || after.starts_with('）'));
        if in_brackets {
            return 20;
        }
    }

    // Substring match — penalize short names to reduce false positives
    if title.contains(name) {
        match name.chars().count() {
            1..=2 => 5,
            3..=4 => 10,
            _ => 15,
        }
    } else {
        0
    }
}

/// Score artist match: 0-20 points.
/// Checks both the channel name (author) and the video title.
fn score_artist(title: &str, author: &str, singer: &str) -> u32 {
    if singer.is_empty() {
        return 0;
    }

    // Exact match in channel name (most authoritative — official channel)
    if author == singer {
        return 20;
    }
    // Exact match in title
    if title == singer {
        return 18;
    }
    // Channel name contains artist
    if author.contains(singer) {
        return 16;
    }
    // Title contains artist
    if title.contains(singer) {
        return 14;
    }

    0
}

/// Score duration match: 0-10 points.
/// Returns 0 (no signal) when NCM duration is unknown.
fn score_duration(ncm_duration_ms: u64, yt_secs: Option<u64>) -> u32 {
    if ncm_duration_ms == 0 {
        return 0;
    }
    let Some(yt_secs) = yt_secs else {
        return 0;
    };

    let ncm_secs = ncm_duration_ms / 1000;
    let diff = ncm_secs.abs_diff(yt_secs);

    match diff {
        0..=3 => 10,
        4..=10 => 7,
        11..=30 => 4,
        _ => 0,
    }
}

/// Score a YouTube search result against the original NCM song.
/// Returns 0-60. Threshold for acceptance is 15.
pub fn score_match(
    title: &str,
    author: &str,
    _views: &str,
    yt_duration_secs: Option<u64>,
    song: &SongInfo,
) -> u32 {
    let name_norm = normalize_for_match(&strip_annotation(&song.name));
    let singer_norm = normalize_for_match(&strip_annotation(&song.singer));
    let title_norm = normalize_for_match(title);
    let author_norm = normalize_for_match(author);

    let mut score = 0u32;
    score += score_name(&title_norm, &name_norm);
    score += score_artist(&title_norm, &author_norm, &singer_norm);
    score += score_duration(song.duration, yt_duration_secs);
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_annotation() {
        assert_eq!(strip_annotation("世界末日(Live)"), "世界末日");
        assert_eq!(strip_annotation("歌曲名 [歌词版]"), "歌曲名");
        assert_eq!(strip_annotation("无标注"), "无标注");
    }

    #[test]
    fn test_parse_duration_str() {
        assert_eq!(parse_duration_str("3:33"), Some(213));
        assert_eq!(parse_duration_str("1:02:33"), Some(3753));
        assert_eq!(parse_duration_str("invalid"), None);
    }

    #[test]
    fn test_score_name() {
        assert_eq!(score_name("世界末日", "世界末日"), 30); // exact
        assert_eq!(score_name("世界末日 周杰伦", "世界末日"), 25); // prefix
        assert_eq!(score_name("周杰伦 世界末日", "世界末日"), 22); // suffix
        assert_eq!(score_name("周杰伦【世界末日】", "世界末日"), 20); // brackets
        assert_eq!(score_name("周杰伦-世界末日-HQ", "世界末日"), 10); // substring 3-4 chars
        assert_eq!(score_name("其他内容", "世界末日"), 0); // no match
    }

    #[test]
    fn test_score_artist() {
        assert_eq!(score_artist("标题", "周杰伦", "周杰伦"), 20); // exact channel
        assert_eq!(score_artist("周杰伦 - 歌曲", "其他频道", "周杰伦"), 14); // in title
        assert_eq!(score_artist("其他", "其他频道", "周杰伦"), 0); // no match
    }

    #[test]
    fn test_cjk_normalization() {
        assert_eq!(normalize_for_match("周杰倫"), "周杰伦");
        assert_eq!(normalize_for_match("周杰倫 Jay Chou"), "周杰伦 jay chou");
    }

    #[test]
    fn test_parse_view_count() {
        assert_eq!(parse_view_count("7,377,641 views"), 7377641);
        assert_eq!(parse_view_count("1,743,047 views"), 1743047);
        assert_eq!(parse_view_count(""), 0);
    }
}
