//! Filename templates: resolving a song to a cache filename and parsing a
//! cached stem back into name/singer for display.

use ncm_api::SongInfo;

use super::super::CacheManager;
use crate::persist::sanitize_filename;

impl CacheManager {
    pub(super) fn resolve_filename(&self, song: &SongInfo, ext: &str) -> String {
        if self.template == "{id}" {
            use std::fmt::Write;
            let mut out = String::with_capacity(16 + ext.len());
            let _ = write!(out, "{}.{}", song.id, ext);
            return out;
        }
        let mut out = String::with_capacity(self.template.len() + ext.len() + 16);
        let mut rest = self.template.as_str();
        while let Some(pos) = rest.find('{') {
            out.push_str(&rest[..pos]);
            let end = rest[pos..]
                .find('}')
                .map(|e| pos + e + 1)
                .unwrap_or(rest.len());
            let placeholder = &rest[pos..end];
            match placeholder {
                "{id}" => {
                    use std::fmt::Write;
                    let _ = write!(out, "{}", song.id);
                }
                "{name}" => out.push_str(&sanitize_filename(&song.name)),
                "{singer}" => out.push_str(&sanitize_filename(&song.singer)),
                "{album}" => out.push_str(&sanitize_filename(&song.album)),
                other => out.push_str(other),
            }
            rest = &rest[end..];
        }
        out.push_str(rest);
        // The song id is the only field guaranteed to be unique. Without it two
        // different songs with the same name/singer (live vs. studio, covers,
        // remixes) resolve to the same file, truncate each other and make the
        // cache index serve the wrong audio. Append it even when the user's
        // template omits `{id}`; old index entries keep their old filenames.
        if !self.template.contains("{id}") {
            use std::fmt::Write;
            let _ = write!(out, "-{}", song.id);
        }
        out.push('.');
        out.push_str(ext);
        out
    }

    /// Parse a cached filename stem into (name, singer) using the template.
    pub(super) fn parse_filename(&self, stem: &str, id: u64) -> (String, String) {
        Self::parse_filename_static(stem, id, &self.template)
    }

    pub(super) fn parse_filename_static(stem: &str, id: u64, template: &str) -> (String, String) {
        if template == "{id}" {
            return (id.to_string(), String::new());
        }

        // `resolve_filename` appends `-<id>` when the template has no `{id}`;
        // strip it so the cached-songs list shows a clean name/singer pair.
        let stem = if template.contains("{id}") {
            stem
        } else {
            stem.strip_suffix(&format!("-{id}")).unwrap_or(stem)
        };

        // Find the last literal separator in the template
        let placeholders = ["{id}", "{name}", "{singer}", "{album}"];
        let mut last_sep_start = 0;
        let mut last_sep_len = 0;
        let mut remaining = template;
        let mut offset = 0;
        while !remaining.is_empty() {
            let mut earliest = remaining.len();
            let mut earliest_len = 0;
            for ph in &placeholders {
                if let Some(pos) = remaining.find(ph)
                    && pos < earliest
                {
                    earliest = pos;
                    earliest_len = ph.len();
                }
            }
            if earliest_len == 0 {
                break;
            }
            if earliest > 0 {
                last_sep_start = offset;
                last_sep_len = earliest;
            }
            remaining = &remaining[earliest + earliest_len..];
            offset += earliest + earliest_len;
        }

        if last_sep_len == 0 {
            return (stem.to_string(), String::new());
        }

        let sep = &template[last_sep_start..last_sep_start + last_sep_len];

        if sep.is_empty() {
            return (stem.to_string(), String::new());
        }

        // Split from the right by the separator
        if let Some(pos) = stem.rfind(sep) {
            let name = stem[..pos].to_string();
            let singer = stem[pos + sep.len()..].to_string();
            return (name, singer);
        }

        (stem.to_string(), String::new())
    }
}

/// Canonical audio-file extension for a raw stem string. Normalises
/// variant spellings (e.g. `mp4`, `m4a`) into the canonical form used as the
/// cache key throughout the codebase. Falls back to `"mp3"` for anything
/// unrecognised.
pub(crate) fn canonical_ext(ext: &str) -> &'static str {
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
