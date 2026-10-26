//! Audio-file helpers shared by the cache and the playback source.

/// Canonical audio-file extension for a raw stem string. Normalises
/// variant spellings (e.g. `mp4`, `m4a`) into the canonical form used as the
/// cache key throughout the codebase. Falls back to `"mp3"` for anything
/// unrecognised.
pub fn canonical_ext(ext: &str) -> &'static str {
    match ext {
        "flac" => "flac",
        "ogg" => "ogg",
        "wav" => "wav",
        "m4a" | "mp4" => "m4a",
        _ => "mp3",
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_ext;

    #[test]
    fn canonical_ext_normalises_variants() {
        assert_eq!(canonical_ext("flac"), "flac");
        assert_eq!(canonical_ext("ogg"), "ogg");
        assert_eq!(canonical_ext("wav"), "wav");
        assert_eq!(canonical_ext("m4a"), "m4a");
        assert_eq!(canonical_ext("mp4"), "m4a");
        assert_eq!(canonical_ext("mp3"), "mp3");
        assert_eq!(canonical_ext("wmv"), "mp3");
    }
}
