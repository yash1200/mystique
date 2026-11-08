//! Cell encoding and Postgres type labels for query / preview results.

use std::error::Error;

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use mystique_connector_api::{json_decimal, json_f64, json_i64, ColumnInfo};
use mystique_sql::{quote_ident, SqlDialect};
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
        Kind::Array(elem) => return decode_array(row, idx, elem),
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
        Type::INT2 => match row.try_get::<_, Option<i16>>(idx) {
            Ok(v) => v.map(|n| json!(n)).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::INT4 => match row.try_get::<_, Option<i32>>(idx) {
            Ok(v) => v.map(|n| json!(n)).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::INT8 => match row.try_get::<_, Option<i64>>(idx) {
            Ok(v) => v.map(json_i64).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::FLOAT4 => match row.try_get::<_, Option<f32>>(idx) {
            Ok(v) => v.map(|n| json_f64(f64::from(n))).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::FLOAT8 => match row.try_get::<_, Option<f64>>(idx) {
            Ok(v) => v.map(json_f64).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::NUMERIC => match row.try_get::<_, Option<PgNumeric>>(idx) {
            Ok(v) => v.map(|n| json_decimal(n.0)).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::INET | Type::CIDR => match row.try_get::<_, Option<PgInet>>(idx) {
            Ok(v) => v.map(|n| JsonValue::String(n.0)).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::INTERVAL => match row.try_get::<_, Option<PgInterval>>(idx) {
            Ok(v) => v.map(|n| JsonValue::String(n.0)).unwrap_or(JsonValue::Null),
            Err(_) => JsonValue::Null,
        },
        Type::MONEY => match row.try_get::<_, Option<PgMoney>>(idx) {
            Ok(v) => v.map(|n| json_decimal(n.0)).unwrap_or(JsonValue::Null),
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

fn decode_array(row: &Row, idx: usize, elem: &Type) -> JsonValue {
    match *elem {
        Type::INT2 => array_of::<i16>(row, idx, |n| json!(n)),
        Type::INT4 => array_of::<i32>(row, idx, |n| json!(n)),
        Type::INT8 => array_of::<i64>(row, idx, json_i64),
        Type::FLOAT4 => array_of::<f32>(row, idx, |n| json_f64(f64::from(n))),
        Type::FLOAT8 => array_of::<f64>(row, idx, json_f64),
        Type::BOOL => array_of::<bool>(row, idx, JsonValue::Bool),
        Type::TEXT | Type::VARCHAR | Type::BPCHAR => {
            array_of::<String>(row, idx, JsonValue::String)
        }
        _ => JsonValue::Null,
    }
}

fn array_of<T>(row: &Row, idx: usize, map: impl Fn(T) -> JsonValue) -> JsonValue
where
    T: for<'a> FromSql<'a>,
{
    match row.try_get::<_, Option<Vec<T>>>(idx) {
        Ok(Some(values)) => JsonValue::Array(values.into_iter().map(map).collect()),
        _ => JsonValue::Null,
    }
}

/// Metrics read the same decoders as grid cells, then narrow to f64.
pub(super) fn metric_f64(row: &Row, idx: usize) -> Option<f64> {
    if let Ok(Some(v)) = row.try_get::<_, Option<f64>>(idx) {
        return Some(v);
    }
    if let Ok(Some(v)) = row.try_get::<_, Option<f32>>(idx) {
        return Some(f64::from(v));
    }
    if let Ok(Some(v)) = row.try_get::<_, Option<i64>>(idx) {
        return Some(v as f64);
    }
    if let Ok(Some(v)) = row.try_get::<_, Option<i32>>(idx) {
        return Some(f64::from(v));
    }
    if let Ok(Some(v)) = row.try_get::<_, Option<i16>>(idx) {
        return Some(f64::from(v));
    }
    if let Ok(Some(PgNumeric(text))) = row.try_get::<_, Option<PgNumeric>>(idx) {
        return text.parse().ok();
    }
    row.try_get::<_, Option<String>>(idx)
        .ok()
        .flatten()
        .and_then(|text| text.parse().ok())
}

struct PgNumeric(String);

impl<'a> FromSql<'a> for PgNumeric {
    fn from_sql(_ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        Ok(Self(numeric_to_string(raw)?))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::NUMERIC)
    }
}

struct PgInet(String);

impl<'a> FromSql<'a> for PgInet {
    fn from_sql(_ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        Ok(Self(inet_to_string(raw)?))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::INET | Type::CIDR)
    }
}

struct PgInterval(String);

impl<'a> FromSql<'a> for PgInterval {
    fn from_sql(_ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        Ok(Self(interval_to_string(raw)?))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::INTERVAL)
    }
}

struct PgMoney(String);

impl<'a> FromSql<'a> for PgMoney {
    fn from_sql(_ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        Ok(Self(money_to_string(raw)?))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::MONEY)
    }
}

fn byte_at(raw: &[u8], at: usize) -> Result<u8, Box<dyn Error + Sync + Send>> {
    raw.get(at)
        .copied()
        .ok_or_else(|| "postgres value truncated".into())
}

fn be_i16(raw: &[u8], at: usize) -> Result<i16, Box<dyn Error + Sync + Send>> {
    Ok(i16::from_be_bytes([
        byte_at(raw, at)?,
        byte_at(raw, at + 1)?,
    ]))
}

fn be_u16(raw: &[u8], at: usize) -> Result<u16, Box<dyn Error + Sync + Send>> {
    Ok(u16::from_be_bytes([
        byte_at(raw, at)?,
        byte_at(raw, at + 1)?,
    ]))
}

fn be_i32(raw: &[u8], at: usize) -> Result<i32, Box<dyn Error + Sync + Send>> {
    Ok(i32::from_be_bytes([
        byte_at(raw, at)?,
        byte_at(raw, at + 1)?,
        byte_at(raw, at + 2)?,
        byte_at(raw, at + 3)?,
    ]))
}

fn be_i64(raw: &[u8], at: usize) -> Result<i64, Box<dyn Error + Sync + Send>> {
    Ok(i64::from_be_bytes([
        byte_at(raw, at)?,
        byte_at(raw, at + 1)?,
        byte_at(raw, at + 2)?,
        byte_at(raw, at + 3)?,
        byte_at(raw, at + 4)?,
        byte_at(raw, at + 5)?,
        byte_at(raw, at + 6)?,
        byte_at(raw, at + 7)?,
    ]))
}

/// Postgres `numeric` binary: base-10000 digits, display scale preserved.
fn numeric_to_string(raw: &[u8]) -> Result<String, Box<dyn Error + Sync + Send>> {
    let ndigits = be_i16(raw, 0)?;
    let weight = be_i16(raw, 2)?;
    let sign = be_u16(raw, 4)?;
    let dscale = be_i16(raw, 6)?;
    match sign {
        0xC000 => return Ok("NaN".into()),
        0xD000 => return Ok("Infinity".into()),
        0xF000 => return Ok("-Infinity".into()),
        0x0000 | 0x4000 => {}
        _ => return Err("unrecognized numeric sign".into()),
    }
    if ndigits < 0 {
        return Err("numeric digit count is negative".into());
    }
    let count = ndigits as usize;
    let mut digits = Vec::with_capacity(count);
    for i in 0..count {
        let digit = be_u16(raw, 8 + i * 2)?;
        if digit >= 10_000 {
            return Err("numeric digit is out of range".into());
        }
        digits.push(digit);
    }
    Ok(format_numeric(&digits, weight, dscale, sign == 0x4000))
}

fn format_numeric(digits: &[u16], weight: i16, dscale: i16, neg: bool) -> String {
    let scale = u16::try_from(dscale).unwrap_or(0).min(1_000) as usize;
    if digits.is_empty() {
        let mut out = if neg {
            "-0".to_string()
        } else {
            "0".to_string()
        };
        if scale > 0 {
            out.push('.');
            out.extend(std::iter::repeat_n('0', scale));
        }
        return out;
    }

    let mut int_buf = String::new();
    let mut frac_buf = String::new();
    if weight < 0 {
        let missing = usize::from(weight.unsigned_abs()) - 1;
        frac_buf.extend(std::iter::repeat_n('0', missing.saturating_mul(4)));
    }
    let mut next_exp = weight;
    for (i, digit) in digits.iter().enumerate() {
        let exp = weight - i16::try_from(i).unwrap_or(i16::MAX);
        while next_exp > exp {
            if next_exp >= 0 {
                int_buf.push_str("0000");
            } else {
                frac_buf.push_str("0000");
            }
            next_exp -= 1;
        }
        let chunk = format!("{digit:04}");
        if exp >= 0 {
            int_buf.push_str(&chunk);
        } else {
            frac_buf.push_str(&chunk);
        }
        next_exp = exp - 1;
    }
    while next_exp >= 0 {
        int_buf.push_str("0000");
        next_exp -= 1;
    }

    let int_trim = int_buf.trim_start_matches('0');
    let int_part = if int_trim.is_empty() { "0" } else { int_trim };
    if frac_buf.len() < scale {
        frac_buf.extend(std::iter::repeat_n('0', scale - frac_buf.len()));
    } else {
        frac_buf.truncate(scale);
    }

    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(int_part);
    if scale > 0 {
        out.push('.');
        out.push_str(&frac_buf);
    }
    out
}

fn inet_to_string(raw: &[u8]) -> Result<String, Box<dyn Error + Sync + Send>> {
    let family = byte_at(raw, 0)?;
    let mask = byte_at(raw, 1)?;
    let len = usize::from(byte_at(raw, 3)?);
    let addr = raw.get(4..4 + len).ok_or("inet address truncated")?;
    let (text, full_mask) = match (family, len) {
        (2, 4) => {
            let a = byte_at(addr, 0)?;
            let b = byte_at(addr, 1)?;
            let c = byte_at(addr, 2)?;
            let d = byte_at(addr, 3)?;
            (format!("{a}.{b}.{c}.{d}"), 32u8)
        }
        (3, 16) => {
            let mut octets = [0u8; 16];
            for (i, slot) in octets.iter_mut().enumerate() {
                *slot = byte_at(addr, i)?;
            }
            (std::net::Ipv6Addr::from(octets).to_string(), 128u8)
        }
        _ => return Err("inet family is not IP".into()),
    };
    if mask == full_mask {
        Ok(text)
    } else {
        Ok(format!("{text}/{mask}"))
    }
}

fn interval_to_string(raw: &[u8]) -> Result<String, Box<dyn Error + Sync + Send>> {
    let micros = be_i64(raw, 0)?;
    let days = be_i32(raw, 8)?;
    let months = be_i32(raw, 12)?;
    Ok(format_interval(months, days, micros))
}

fn format_interval(months: i32, days: i32, micros: i64) -> String {
    let mut parts = Vec::new();
    if months != 0 {
        let label = if months.unsigned_abs() == 1 {
            "mon"
        } else {
            "mons"
        };
        parts.push(format!("{months} {label}"));
    }
    if days != 0 {
        let label = if days.unsigned_abs() == 1 {
            "day"
        } else {
            "days"
        };
        parts.push(format!("{days} {label}"));
    }
    if micros != 0 || parts.is_empty() {
        let negative = micros < 0;
        let us = micros.unsigned_abs();
        let total_secs = us / 1_000_000;
        let frac = us % 1_000_000;
        let hours = total_secs / 3_600;
        let mins = (total_secs % 3_600) / 60;
        let secs = total_secs % 60;
        let mut time = format!("{hours:02}:{mins:02}:{secs:02}");
        if frac != 0 {
            let mut fraction = format!("{frac:06}");
            while fraction.ends_with('0') {
                fraction.pop();
            }
            time.push('.');
            time.push_str(&fraction);
        }
        if negative {
            time.insert(0, '-');
        }
        parts.push(time);
    }
    parts.join(" ")
}

fn money_to_string(raw: &[u8]) -> Result<String, Box<dyn Error + Sync + Send>> {
    let cents = be_i64(raw, 0)?;
    let neg = cents < 0;
    let abs = cents.unsigned_abs();
    let whole = abs / 100;
    let frac = abs % 100;
    let mut out = format!("{whole}.{frac:02}");
    if neg {
        out.insert(0, '-');
    }
    Ok(out)
}

pub(super) fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(super) fn preview_select_list(style: SqlDialect, columns: &[ColumnInfo]) -> String {
    if columns.is_empty() {
        return "*".into();
    }
    columns
        .iter()
        .map(|c| quote_ident(style, &c.name))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn preview_order_sql(style: SqlDialect, columns: &[ColumnInfo]) -> String {
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

    fn be_push_i16(buf: &mut Vec<u8>, n: i16) {
        buf.extend(n.to_be_bytes());
    }

    fn be_push_u16(buf: &mut Vec<u8>, n: u16) {
        buf.extend(n.to_be_bytes());
    }

    #[test]
    fn numeric_round_trips_an_exact_decimal() {
        let mut raw = Vec::new();
        be_push_i16(&mut raw, 6);
        be_push_i16(&mut raw, 4);
        be_push_u16(&mut raw, 0);
        be_push_i16(&mut raw, 2);
        for digit in [1234u16, 5678, 9012, 3456, 7890, 2500] {
            be_push_u16(&mut raw, digit);
        }
        assert_eq!(
            numeric_to_string(&raw).expect("numeric"),
            "12345678901234567890.25"
        );
    }

    #[test]
    fn numeric_small_values_keep_scale() {
        let mut raw = Vec::new();
        be_push_i16(&mut raw, 2);
        be_push_i16(&mut raw, 0);
        be_push_u16(&mut raw, 0x4000);
        be_push_i16(&mut raw, 2);
        be_push_u16(&mut raw, 123);
        be_push_u16(&mut raw, 4500);
        assert_eq!(numeric_to_string(&raw).expect("numeric"), "-123.45");
    }

    #[test]
    fn interval_day_and_inet_host_are_text() {
        let mut raw = Vec::new();
        raw.extend(0i64.to_be_bytes());
        raw.extend(1i32.to_be_bytes());
        raw.extend(0i32.to_be_bytes());
        assert_eq!(interval_to_string(&raw).expect("interval"), "1 day");

        let inet = [2u8, 32, 0, 4, 127, 0, 0, 1];
        assert_eq!(inet_to_string(&inet).expect("inet"), "127.0.0.1");
        assert_eq!(
            money_to_string(&125i64.to_be_bytes()).expect("money"),
            "1.25"
        );
    }
}
