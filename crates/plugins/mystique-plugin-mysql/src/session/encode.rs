//! Cell encoding and MySQL type labels for query / preview results.

use mysql_async::consts::ColumnType;
use mysql_async::Value;
use mystique_connector_api::{json_f64, json_i64, json_u64};
use serde_json::Value as JsonValue;

/// SQL-ish type name for query result headers (not Debug enum tags).
pub(super) fn mysql_type_label(t: ColumnType) -> &'static str {
    use ColumnType::*;
    match t {
        MYSQL_TYPE_DECIMAL | MYSQL_TYPE_NEWDECIMAL => "decimal",
        MYSQL_TYPE_TINY => "tinyint",
        MYSQL_TYPE_SHORT => "smallint",
        MYSQL_TYPE_LONG => "int",
        MYSQL_TYPE_FLOAT => "float",
        MYSQL_TYPE_DOUBLE => "double",
        MYSQL_TYPE_NULL => "null",
        MYSQL_TYPE_TIMESTAMP | MYSQL_TYPE_TIMESTAMP2 => "timestamp",
        MYSQL_TYPE_LONGLONG => "bigint",
        MYSQL_TYPE_INT24 => "mediumint",
        MYSQL_TYPE_DATE | MYSQL_TYPE_NEWDATE => "date",
        MYSQL_TYPE_TIME | MYSQL_TYPE_TIME2 => "time",
        MYSQL_TYPE_DATETIME | MYSQL_TYPE_DATETIME2 => "datetime",
        MYSQL_TYPE_YEAR => "year",
        MYSQL_TYPE_VARCHAR => "varchar",
        MYSQL_TYPE_BIT => "bit",
        MYSQL_TYPE_JSON => "json",
        MYSQL_TYPE_ENUM => "enum",
        MYSQL_TYPE_SET => "set",
        MYSQL_TYPE_TINY_BLOB => "tinyblob",
        MYSQL_TYPE_MEDIUM_BLOB => "mediumblob",
        MYSQL_TYPE_LONG_BLOB => "longblob",
        MYSQL_TYPE_BLOB => "blob",
        MYSQL_TYPE_VAR_STRING => "varchar",
        MYSQL_TYPE_STRING => "char",
        MYSQL_TYPE_GEOMETRY => "geometry",
        MYSQL_TYPE_VECTOR => "vector",
        MYSQL_TYPE_TYPED_ARRAY => "array",
        MYSQL_TYPE_UNKNOWN => "unknown",
    }
}

pub(super) fn value_json(v: Value) -> JsonValue {
    match v {
        Value::NULL => JsonValue::Null,
        Value::Bytes(b) => match String::from_utf8(b.clone()) {
            Ok(s) => JsonValue::String(s),
            Err(_) => JsonValue::String(format!(
                "\\x{}",
                b.iter().map(|x| format!("{x:02x}")).collect::<String>()
            )),
        },
        Value::Int(i) => json_i64(i),
        Value::UInt(u) => json_u64(u),
        Value::Float(f) => json_f64(f64::from(f)),
        Value::Double(d) => json_f64(d),
        // DATE / DATETIME / TIMESTAMP all arrive as Value::Date
        Value::Date(year, month, day, hour, minute, second, micros) => {
            // MySQL zero-date / invalid components
            if year == 0 || month == 0 || day == 0 {
                return JsonValue::Null;
            }
            // Pure DATE (no time component)
            if hour == 0 && minute == 0 && second == 0 && micros == 0 {
                // Could still be midnight DATETIME — still valid ISO form.
                // Prefer full datetime when any time field is non-zero; for
                // pure DATE columns this matches YYYY-MM-DD which is correct.
                return JsonValue::String(format!("{year:04}-{month:02}-{day:02}"));
            }
            if micros == 0 {
                JsonValue::String(format!(
                    "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"
                ))
            } else {
                // Trim trailing zeros in fractional seconds for readability
                let frac = format!("{micros:06}");
                let frac = frac.trim_end_matches('0');
                JsonValue::String(format!(
                    "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{frac}"
                ))
            }
        }
        Value::Time(is_neg, days, hours, minutes, seconds, micros) => {
            let sign = if is_neg { "-" } else { "" };
            if micros == 0 {
                JsonValue::String(format!("{sign}{days} {hours:02}:{minutes:02}:{seconds:02}"))
            } else {
                let frac = format!("{micros:06}");
                let frac = frac.trim_end_matches('0');
                JsonValue::String(format!(
                    "{sign}{days} {hours:02}:{minutes:02}:{seconds:02}.{frac}"
                ))
            }
        }
    }
}

/// Present DEFAULT / AUTO_INCREMENT for structure UI.
pub(super) fn mysql_default_display(column_default: Option<&str>, extra: &str) -> Option<String> {
    let extra = extra.trim();
    let mut parts = Vec::new();
    if let Some(d) = column_default.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(d.to_string());
    }
    if !extra.is_empty() {
        // e.g. auto_increment, on update CURRENT_TIMESTAMP
        parts.push(extra.to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

pub(super) fn parse_mysql_enum_values(column_type: &str) -> Option<Vec<String>> {
    let lower = column_type.to_ascii_lowercase();
    if !(lower.starts_with("enum(") || lower.starts_with("set(")) {
        return None;
    }
    let mut values = Vec::new();
    let mut chars = column_type.chars();
    while let Some(ch) = chars.next() {
        if ch != '\'' {
            continue;
        }
        let mut s = String::new();
        loop {
            match chars.next() {
                None => break,
                Some('\\') => {
                    if let Some(next) = chars.next() {
                        s.push(next);
                    }
                }
                Some('\'') => break,
                Some(c) => s.push(c),
            }
        }
        values.push(s);
    }
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigint_outside_the_safe_range_is_a_string() {
        assert_eq!(
            value_json(Value::Int(9_007_199_254_740_993)),
            JsonValue::String("9007199254740993".into())
        );
        assert_eq!(value_json(Value::Int(42)), serde_json::json!(42));
        assert_eq!(
            value_json(Value::Bytes(b"12345678901234567890.25".to_vec())),
            JsonValue::String("12345678901234567890.25".into())
        );
    }

    #[test]
    fn enum_values_keep_utf8() {
        let values = parse_mysql_enum_values("enum('日本','Zürich')").expect("enum");
        assert_eq!(values, vec!["日本".to_string(), "Zürich".to_string()]);
    }
}
