//! JSON cells that survive `JSON.parse` in the grid.
//!
//! JavaScript numbers are f64. Integers outside ±(2^53−1) and every
//! numeric/decimal value are JSON strings so a later `WHERE` still names the
//! same row. The column `data_type` tag (`int8`, `numeric`, `text`) is how the
//! grid tells an id-string from ordinary text.

use serde_json::{Number, Value as JsonValue};

/// `Number.MAX_SAFE_INTEGER`.
pub const JS_MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// `int2` / `int4` / in-range `int8` stay JSON numbers. The rest are decimal text.
pub fn json_i64(n: i64) -> JsonValue {
    if (-JS_MAX_SAFE_INTEGER..=JS_MAX_SAFE_INTEGER).contains(&n) {
        JsonValue::Number(Number::from(n))
    } else {
        JsonValue::String(n.to_string())
    }
}

/// Unsigned counterpart of [`json_i64`]. MySQL `BIGINT UNSIGNED` uses this.
pub fn json_u64(n: u64) -> JsonValue {
    if n <= JS_MAX_SAFE_INTEGER as u64 {
        JsonValue::Number(Number::from(n))
    } else {
        JsonValue::String(n.to_string())
    }
}

/// NUMERIC / DECIMAL always cross IPC as exact decimal text.
pub fn json_decimal(text: impl Into<String>) -> JsonValue {
    JsonValue::String(text.into())
}

/// Finite floats stay numbers. NaN and infinities are text, because JSON has none.
pub fn json_f64(n: f64) -> JsonValue {
    Number::from_f64(n).map_or_else(
        || {
            let label = if n.is_nan() {
                "NaN"
            } else if n.is_sign_positive() {
                "Infinity"
            } else {
                "-Infinity"
            };
            JsonValue::String(label.into())
        },
        JsonValue::Number,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn safe_ints_stay_numbers_and_unsafe_ints_are_quoted() {
        assert_eq!(json_i64(42), json!(42));
        assert_eq!(json_i64(JS_MAX_SAFE_INTEGER), json!(JS_MAX_SAFE_INTEGER));
        assert_eq!(json_i64(-JS_MAX_SAFE_INTEGER), json!(-JS_MAX_SAFE_INTEGER));
        assert_eq!(
            json_i64(9_007_199_254_740_993),
            JsonValue::String("9007199254740993".into())
        );
        assert_eq!(
            json_i64(i64::MAX),
            JsonValue::String("9223372036854775807".into())
        );
        assert_eq!(
            json_i64(i64::MIN),
            JsonValue::String("-9223372036854775808".into())
        );
        let payload = json!({ "id": json_i64(9_007_199_254_740_993) });
        let text = serde_json::to_string(&payload).expect("json");
        assert!(text.contains("\"9007199254740993\""), "{text}");
        assert!(!text.contains("9007199254740992"), "{text}");
    }

    #[test]
    fn unsigned_boundary_matches_the_signed_safe_range() {
        assert_eq!(json_u64(42), json!(42));
        assert_eq!(json_u64(u64::MAX), JsonValue::String(u64::MAX.to_string()));
    }

    #[test]
    fn decimals_are_strings_and_non_finite_floats_are_not_numbers() {
        assert_eq!(
            json_decimal("12345678901234567890.25"),
            JsonValue::String("12345678901234567890.25".into())
        );
        assert_eq!(json_f64(1.5), json!(1.5));
        assert_eq!(json_f64(f64::NAN), JsonValue::String("NaN".into()));
    }
}
