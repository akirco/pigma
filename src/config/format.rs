//! TOML re-serialization helpers used when writing the config file: flatten
//! array-of-tables sections into inline tables so the output stays compact.

use toml_edit::{Array, InlineTable, Table, Value};

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
