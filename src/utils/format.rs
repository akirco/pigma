use toml_edit::{Array, InlineTable, Table, Value};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Convert a named field in the table from an array of tables into a multi-line inline table array.
///
/// # Arguments
/// - `table`: the table to modify (e.g. a section)
/// - `key`: the field name (e.g. "items")
/// - `indent`: the indentation before each inline table (e.g. "\n  ")
pub fn convert_aot_to_inline(table: &mut Table, key: &str, indent: &str) -> bool {
    let item = match table.remove(key) {
        Some(item) => item,
        None => return false,
    };

    let aot = match item.as_array_of_tables() {
        Some(aot) => aot,
        None => {
            table.insert(key, item); // Not an array of tables, so put it back
            return false;
        }
    };

    let mut arr = Array::new();
    for child in aot.iter() {
        let mut inline = InlineTable::new();
        for (k, v) in child.iter() {
            if let Some(val) = v.as_value() {
                inline.insert(k, val.clone());
            }
        }
        inline.fmt();
        let mut v = Value::InlineTable(inline);
        v.decor_mut().set_prefix(indent);
        arr.push_formatted(v);
    }
    arr.set_trailing("\n");

    table.insert(key, Value::Array(arr).into());
    true
}

/// Convert all array-of-tables fields in the table into inline table arrays.
pub fn convert_all_aot_to_inline(table: &mut Table, indent: &str) {
    // Collect the keys to convert first (cannot modify while iterating)
    let keys: Vec<String> = table
        .iter()
        .filter(|(_, item)| item.is_array_of_tables())
        .map(|(key, _)| key.to_string())
        .collect();

    for key in keys {
        convert_aot_to_inline(table, &key, indent);
    }
}

/// Cap a tab label to `max_cells` display cells, appending an ellipsis when
/// truncated, so a single long playlist name can't push the others off screen.
pub fn clip_long_text(s: &str, max_cells: usize) -> String {
    if UnicodeWidthStr::width(s) <= max_cells {
        return s.to_string();
    }
    let mut out = String::new();
    let mut cells = 0;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if cells + cw > max_cells.saturating_sub(1) {
            break;
        }
        out.push(ch);
        cells += cw;
    }
    out.push('…');
    out
}

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
    use super::*;

    #[test]
    fn clip_keeps_short_labels() {
        assert_eq!(clip_long_text("我喜欢的音乐", 24), "我喜欢的音乐");
        assert_eq!(clip_long_text("short", 24), "short");
    }

    #[test]
    fn clip_truncates_wide_labels() {
        let clipped = clip_long_text("一个特别特别特别长的歌单名字", 8);
        assert!(clipped.ends_with('…'));
        assert!(UnicodeWidthStr::width(clipped.as_str()) <= 8);
    }

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
