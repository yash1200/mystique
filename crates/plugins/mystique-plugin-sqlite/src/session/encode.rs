//! Cell encoding for SQLite query / preview results.

use mystique_connector_api::{json_f64, json_i64};
use rusqlite::types::ValueRef;
use serde_json::Value as JsonValue;

pub(super) fn value_ref_json(v: ValueRef<'_>) -> JsonValue {
    match v {
        ValueRef::Null => JsonValue::Null,
        ValueRef::Integer(i) => json_i64(i),
        ValueRef::Real(f) => json_f64(f),
        ValueRef::Text(t) => JsonValue::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => JsonValue::String(format!(
            "\\x{}",
            b.iter().map(|x| format!("{x:02x}")).collect::<String>()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_outside_the_safe_range_is_a_string() {
        assert_eq!(
            value_ref_json(ValueRef::Integer(9_007_199_254_740_993)),
            JsonValue::String("9007199254740993".into())
        );
        assert_eq!(value_ref_json(ValueRef::Integer(42)), serde_json::json!(42));
    }
}
