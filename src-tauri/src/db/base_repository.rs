//! Generic repository helpers.
//!
//! The concrete repositories use `Arc<Mutex<Connection>>` (SharedDb) for
//! thread-safe access; this module provides shared row-conversion helpers.

use std::collections::HashMap;

use rusqlite::types::Value;

pub type SqlRow = HashMap<String, Value>;

/// Convert a rusqlite row into a `SqlRow` map of string-keyed values.
pub fn row_to_map(row: &rusqlite::Row) -> rusqlite::Result<SqlRow> {
    let mut map = SqlRow::new();
    for (i, column) in row.as_ref().column_names().iter().enumerate() {
        let value: Value = row.get(i)?;
        map.insert(column.to_string(), value);
    }
    Ok(map)
}

/// Convert a `SqlRow` value to a string.
pub fn row_str(row: &SqlRow, key: &str) -> String {
    match row.get(key) {
        Some(Value::Text(s)) => s.clone(),
        Some(Value::Integer(i)) => i.to_string(),
        Some(Value::Real(f)) => f.to_string(),
        _ => String::new(),
    }
}

pub fn row_i64(row: &SqlRow, key: &str) -> i64 {
    match row.get(key) {
        Some(Value::Integer(i)) => *i,
        Some(Value::Text(s)) => s.parse().unwrap_or(0),
        Some(Value::Real(f)) => *f as i64,
        _ => 0,
    }
}

pub fn row_bool(row: &SqlRow, key: &str) -> bool {
    row_i64(row, key) != 0
}

pub fn row_opt_i64(row: &SqlRow, key: &str) -> Option<i64> {
    match row.get(key) {
        Some(Value::Integer(i)) => Some(*i),
        Some(Value::Text(s)) => s.parse().ok(),
        _ => None,
    }
}

pub fn row_opt_string(row: &SqlRow, key: &str) -> Option<String> {
    match row.get(key) {
        Some(Value::Text(s)) => Some(s.clone()),
        _ => None,
    }
}

pub fn row_json<T: serde::de::DeserializeOwned>(row: &SqlRow, key: &str) -> Option<T> {
    let raw = row_str(row, key);
    if raw.is_empty() {
        return None;
    }
    serde_json::from_str(&raw).ok()
}
