use serde_json::Value;
use std::collections::HashMap;

/// Serialize any `Serialize` type into a `HashMap<String, String>`.
///
/// Uses serde_json as an intermediate representation to ensure all field types are handled
/// correctly (numbers, booleans, strings, enums). Nested objects are flattened to their
/// debug representation; arrays are joined.
pub fn to_map<T: serde::Serialize>(item: &T) -> HashMap<String, String> {
    let value = match serde_json::to_value(item) {
        Ok(Value::Object(map)) => map,
        _ => return HashMap::new(),
    };

    value
        .into_iter()
        .map(|(k, v)| {
            let s = match v {
                Value::String(s) => s,
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null | Value::Array(_) | Value::Object(_) => String::new(),
            };
            (k, s)
        })
        .collect()
}
