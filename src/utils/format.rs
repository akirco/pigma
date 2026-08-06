use toml_edit::{Array, InlineTable, Table, Value};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// 将表中指定字段从 ArrayOfTables 转为多行内联表数组
///
/// # 参数
/// - `table`: 要修改的表（如某个 section）
/// - `key`: 字段名（如 "items"）
/// - `indent`: 每个内联表前的缩进（如 "\n  "）
pub fn convert_aot_to_inline(table: &mut Table, key: &str, indent: &str) -> bool {
    let item = match table.remove(key) {
        Some(item) => item,
        None => return false,
    };

    let aot = match item.as_array_of_tables() {
        Some(aot) => aot,
        None => {
            table.insert(key, item); // 不是 ArrayOfTables，放回去
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

/// 将表中所有 ArrayOfTables 字段转为内联表数组
pub fn convert_all_aot_to_inline(table: &mut Table, indent: &str) {
    // 先收集需要转换的 key（不能在遍历时修改）
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
}
