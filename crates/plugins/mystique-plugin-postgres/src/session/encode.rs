//! Cell encoding and Postgres type labels for query / preview results.

use std::error::Error;

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use mystique_connector_api::{quote_ident, ColumnInfo, SqlIdentStyle};
use serde_json::{json, Value as JsonValue};
use tokio_postgres::types::{FromSql, Kind, Type};
use tokio_postgres::Row;
use uuid::Uuid;

/// Postgres enum labels are sent as UTF-8. `String` only accepts text/varchar,
/// so custom enums (`panel_access_team`, …) used to decode as NULL.
struct PgEnumLabel(String);

impl<'a> FromSql<'a> for PgEnumLabel {
    fn from_sql(_ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        Ok(Self(std::str::from_utf8(raw)?.to_owned()))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(ty.kind(), Kind::Enum(_))
    }
}

pub(super) fn opt_string(s: Option<String>) -> JsonValue {
    s.map(JsonValue::String).unwrap_or(JsonValue::Null)
}

/// Map information_schema prose → PostgreSQL udt-style developer labels.
pub(super) fn pg_info_schema_to_udt(data_type: &str) -> String {
    match data_type.to_ascii_lowercase().as_str() {
        "timestamp without time zone" => "timestamp".into(),
        "timestamp with time zone" => "timestamptz".into(),
        "time without time zone" => "time".into(),
        "time with time zone" => "timetz".into(),
        "character varying" => "varchar".into(),
        "character" => "char".into(),
        "double precision" => "float8".into(),
        "bit varying" => "varbit".into(),
        "boolean" => "bool".into(),
        "integer" => "int4".into(),
        "smallint" => "int2".into(),
        "bigint" => "int8".into(),
        "real" => "float4".into(),
        "decimal" => "numeric".into(),
        other => other.to_string(),
    }
}

/// Developer-facing PG type: `timestamptz`, `varchar(255)`, `numeric(12,2)`, `int4[]`.
pub(super) fn pg_type_label(
    udt_name: &str,
    data_type: &str,
    char_max: Option<i32>,
    num_prec: Option<i32>,
    num_scale: Option<i32>,
    dt_prec: Option<i32>,
) -> String {
    let mut udt = udt_name.trim().to_string();
    if udt.is_empty() {
        udt = pg_info_schema_to_udt(data_type);
    }
    if udt.is_empty() {
        return "unknown".into();
    }
    // Arrays: `_int4` → `int4[]`
    if let Some(elem) = udt.strip_prefix('_') {
        return format!("{elem}[]");
    }
    if matches!(udt.as_str(), "varchar" | "bpchar" | "bit" | "varbit") {
        if let Some(n) = char_max {
            let base = if udt == "bpchar" {
                "char"
            } else {
                udt.as_str()
            };
            return format!("{base}({n})");
        }
        if udt == "bpchar" {
            return "char".into();
        }
    }
    if udt == "numeric" || udt == "decimal" {
        return match (num_prec, num_scale) {
            (Some(p), Some(s)) => format!("numeric({p},{s})"),
            (Some(p), None) => format!("numeric({p})"),
            _ => "numeric".into(),
        };
    }
    // Default fractional precision is 6 — omit unless non-default.
    if matches!(
        udt.as_str(),
        "timestamp" | "timestamptz" | "time" | "timetz"
    ) {
        if let Some(p) = dt_prec {
            if p != 6 {
                return format!("{udt}({p})");
            }
        }
    }
    udt
}

pub(super) fn pg_opt_i32(v: Option<i32>) -> Option<i32> {
    v
}

/// Format naive timestamp without trailing empty fractional part.
pub(super) fn fmt_naive_dt(dt: NaiveDateTime) -> String {
    let s = dt.format("%Y-%m-%d %H:%M:%S%.6f").to_string();
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub(super) fn fmt_naive_time(t: NaiveTime) -> String {
    let s = t.format("%H:%M:%S%.6f").to_string();
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub(super) fn cell_json(row: &Row, idx: usize) -> JsonValue {
    match row.columns().get(idx) {
        Some(column) => decode_cell(row, idx, column.type_()),
        None => JsonValue::Null,
    }
}

fn decode_cell(row: &Row, idx: usize, ty: &Type) -> JsonValue {
    match ty.kind() {
        Kind::Domain(inner) => return decode_cell(row, idx, inner),
        Kind::Enum(_) => {
            return match row.try_get::<_, Option<PgEnumLabel>>(idx) {
                Ok(v) => opt_string(v.map(|e| e.0)),
                Err(_) => JsonValue::Null,
            };
        }
        Kind::Array(elem) if matches!(elem.kind(), Kind::Enum(_)) => {
            return match row.try_get::<_, Option<Vec<PgEnumLabel>>>(idx) {
                Ok(Some(v)) => {
                    JsonValue::Array(v.into_iter().map(|e| JsonValue::String(e.0)).collect())
                }
                Ok(None) => JsonValue::Null,
                Err(_) => JsonValue::Null,
            };
        }
        _ => {}
    }

    match *ty {
        Type::UUID => match row.try_get::<_, Option<Uuid>>(idx) {
            Ok(v) => v
                .map(|u| JsonValue::String(u.to_string()))
                .unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::UUID_ARRAY => match row.try_get::<_, Option<Vec<Uuid>>>(idx) {
            Ok(Some(v)) => JsonValue::Array(
                v.into_iter()
                    .map(|u| JsonValue::String(u.to_string()))
                    .collect(),
            ),
            Ok(None) => JsonValue::Null,
            Err(_) => JsonValue::Null,
        },
        Type::BOOL => match row.try_get::<_, Option<bool>>(idx) {
            Ok(v) => v.map(JsonValue::Bool).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::INT2 | Type::INT4 => match row.try_get::<_, Option<i32>>(idx) {
            Ok(v) => v.map(|n| json!(n)).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::INT8 => match row.try_get::<_, Option<i64>>(idx) {
            Ok(v) => v.map(|n| json!(n)).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::FLOAT4 | Type::FLOAT8 => match row.try_get::<_, Option<f64>>(idx) {
            Ok(v) => v.map(|n| json!(n)).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::JSON | Type::JSONB => match row.try_get::<_, Option<JsonValue>>(idx) {
            Ok(v) => v.unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        // Timestamps / dates require chrono FromSql (with-chrono-0_4). Reading as
        // String always fails → previously rendered as NULL.
        Type::TIMESTAMP => match row.try_get::<_, Option<NaiveDateTime>>(idx) {
            Ok(v) => v
                .map(|dt| JsonValue::String(fmt_naive_dt(dt)))
                .unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::TIMESTAMPTZ => match row.try_get::<_, Option<DateTime<Utc>>>(idx) {
            Ok(v) => v
                .map(|dt| JsonValue::String(dt.to_rfc3339()))
                .unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::DATE => match row.try_get::<_, Option<NaiveDate>>(idx) {
            Ok(v) => v
                .map(|d| JsonValue::String(d.format("%Y-%m-%d").to_string()))
                .unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::TIME => match row.try_get::<_, Option<NaiveTime>>(idx) {
            Ok(v) => v
                .map(|t| JsonValue::String(fmt_naive_time(t)))
                .unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::NUMERIC => {
            // Prefer text form so precision is preserved.
            match row.try_get::<_, Option<String>>(idx) {
                Ok(v) => opt_string(v),
                Err(_) => match row.try_get::<_, Option<f64>>(idx) {
                    Ok(v) => v.map(|n| json!(n)).unwrap_or(JsonValue::Null),
                    Err(_) => JsonValue::Null,
                },
            }
        }
        _ => match row.try_get::<_, Option<String>>(idx) {
            Ok(v) => opt_string(v),
            Err(_) => {
                // Last resort: chrono types (domains / unknown aliases) + bytes.
                if let Ok(v) = row.try_get::<_, Option<DateTime<Utc>>>(idx) {
                    return v
                        .map(|dt| JsonValue::String(dt.to_rfc3339()))
                        .unwrap_or(JsonValue::Null);
                }
                if let Ok(v) = row.try_get::<_, Option<NaiveDateTime>>(idx) {
                    return v
                        .map(|dt| JsonValue::String(fmt_naive_dt(dt)))
                        .unwrap_or(JsonValue::Null);
                }
                if let Ok(v) = row.try_get::<_, Option<NaiveDate>>(idx) {
                    return v
                        .map(|d| JsonValue::String(d.format("%Y-%m-%d").to_string()))
                        .unwrap_or(JsonValue::Null);
                }
                if let Ok(v) = row.try_get::<_, Option<NaiveTime>>(idx) {
                    return v
                        .map(|t| JsonValue::String(fmt_naive_time(t)))
                        .unwrap_or(JsonValue::Null);
                }
                match row.try_get::<_, Option<Vec<u8>>>(idx) {
                    Ok(Some(b)) => {
                        // UUID binary / text-as-bytes etc.
                        if let Ok(s) = std::str::from_utf8(&b) {
                            JsonValue::String(s.to_string())
                        } else {
                            JsonValue::String(format!("\\x{}", encode_hex(&b)))
                        }
                    }
                    Ok(None) => JsonValue::Null,
                    Err(_) => JsonValue::Null,
                }
            }
        },
    }
}

pub(super) fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(super) fn preview_select_list(style: SqlIdentStyle, columns: &[ColumnInfo]) -> String {
    if columns.is_empty() {
        return "*".into();
    }
    columns
        .iter()
        .map(|c| quote_ident(style, &c.name))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn preview_order_sql(style: SqlIdentStyle, columns: &[ColumnInfo]) -> String {
    let pks: Vec<String> = columns
        .iter()
        .filter(|c| c.pk)
        .map(|c| quote_ident(style, &c.name))
        .collect();
    if !pks.is_empty() {
        return format!(" ORDER BY {}", pks.join(", "));
    }
    if let Some(first) = columns.first() {
        return format!(" ORDER BY {}", quote_ident(style, &first.name));
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_enum_label_decodes_utf8() {
        let label = PgEnumLabel::from_sql(&Type::TEXT, b"admin").expect("label");
        assert_eq!(label.0, "admin");
    }

    #[test]
    fn pg_enum_label_rejects_text_oid() {
        assert!(!<PgEnumLabel as FromSql>::accepts(&Type::TEXT));
        assert!(!<PgEnumLabel as FromSql>::accepts(&Type::UUID));
    }

    #[test]
    fn uuid_display_is_hyphenated() {
        let u = Uuid::parse_str("5f6083e8-922c-4a8a-b6b7-950a17f20d2b").unwrap();
        assert_eq!(u.to_string(), "5f6083e8-922c-4a8a-b6b7-950a17f20d2b");
    }
}
