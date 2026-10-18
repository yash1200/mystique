//! Grid insert / update / delete. One transaction, one statement per row.

use bytes::{BufMut, BytesMut};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use mystique_connector_api::{
    affected_mismatch, keyless_ambiguous, overlay_catalog_pk, plan_table_edits,
    ApplyTableEditsInput, ApplyTableEditsResult, ConnectorError, ConnectorResult, EditPlan,
    SqlIdentStyle, SqlPlaceholder,
};
use serde_json::Value as JsonValue;
use tokio_postgres::types::{IsNull, ToSql, Type};
use uuid::Uuid;

use super::catalog;
use super::query_failed;
use super::PostgresSession;

pub(super) async fn apply_table_edits(
    session: &PostgresSession,
    edits: &ApplyTableEditsInput,
) -> ConnectorResult<ApplyTableEditsResult> {
    session.ensure_open()?;
    let described = catalog::describe_table(
        session,
        edits.database.as_deref(),
        edits.schema.as_deref(),
        &edits.table,
    )
    .await?;
    let mut owned = edits.clone();
    overlay_catalog_pk(&mut owned.columns, &described);
    let plan = plan_table_edits(SqlIdentStyle::Postgres, SqlPlaceholder::Dollar, &owned)?;
    let previews = plan.previews();
    let rollback_sql = plan.rollback_sql();
    if edits.dry_run {
        return Ok(ApplyTableEditsResult {
            updated: 0,
            inserted: 0,
            deleted: 0,
            statements: previews,
            rollback_sql,
        });
    }
    let mut client = session.client().await?;
    let tx = client.transaction().await.map_err(query_failed)?;
    if let Err(err) = apply_plan(&tx, &plan).await {
        let _ = tx.rollback().await;
        return Err(err);
    }
    tx.commit().await.map_err(query_failed)?;
    Ok(ApplyTableEditsResult {
        updated: owned.updates.len() as u32,
        inserted: owned.inserts.len() as u32,
        deleted: owned.deletes.len() as u32,
        statements: previews,
        rollback_sql,
    })
}

pub(super) async fn apply_rollback_sql(
    session: &PostgresSession,
    statements: &[String],
) -> ConnectorResult<()> {
    session.ensure_open()?;
    let mut client = session.client().await?;
    let tx = client.transaction().await.map_err(query_failed)?;
    for sql in statements {
        let changed = match tx.execute(sql.as_str(), &[]).await {
            Ok(changed) => changed,
            Err(err) => {
                let _ = tx.rollback().await;
                return Err(query_failed(err));
            }
        };
        if changed != 1 {
            let _ = tx.rollback().await;
            return Err(affected_mismatch("rollback", changed));
        }
    }
    tx.commit().await.map_err(query_failed)?;
    Ok(())
}

async fn apply_plan(tx: &tokio_postgres::Transaction<'_>, plan: &EditPlan) -> ConnectorResult<()> {
    for edit in &plan.edits {
        if let Some(count) = &edit.uniqueness {
            let found = count_rows(tx, &count.sql, &count.params).await?;
            if found != 1 {
                return Err(if found > 1 {
                    keyless_ambiguous(found)
                } else {
                    affected_mismatch("write", 0)
                });
            }
        }
        let changed = exec_params(tx, &edit.sql, &edit.params).await?;
        if changed != 1 {
            return Err(affected_mismatch(statement_kind(&edit.sql), changed));
        }
    }
    Ok(())
}

async fn count_rows(
    tx: &tokio_postgres::Transaction<'_>,
    sql: &str,
    params: &[JsonValue],
) -> ConnectorResult<i64> {
    let binds = pg_binds(params);
    let refs = pg_refs(&binds);
    let row = tx.query_one(sql, &refs).await.map_err(query_failed)?;
    row.try_get(0)
        .map_err(|err| ConnectorError::Query(err.to_string()))
}

async fn exec_params(
    tx: &tokio_postgres::Transaction<'_>,
    sql: &str,
    params: &[JsonValue],
) -> ConnectorResult<u64> {
    let binds = pg_binds(params);
    let refs = pg_refs(&binds);
    tx.execute(sql, &refs).await.map_err(query_failed)
}

fn pg_binds(params: &[JsonValue]) -> Vec<PgBind> {
    params.iter().cloned().map(PgBind).collect()
}

fn pg_refs(binds: &[PgBind]) -> Vec<&(dyn ToSql + Sync)> {
    binds
        .iter()
        .map(|bind| bind as &(dyn ToSql + Sync))
        .collect()
}

fn statement_kind(sql: &str) -> &'static str {
    let trimmed = sql.trim_start();
    if trimmed.starts_with("INSERT") || trimmed.starts_with("insert") {
        "INSERT"
    } else if trimmed.starts_with("DELETE") || trimmed.starts_with("delete") {
        "DELETE"
    } else {
        "UPDATE"
    }
}

#[derive(Debug)]
struct PgBind(JsonValue);

impl ToSql for PgBind {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        if self.0.is_null() {
            return Ok(IsNull::Yes);
        }
        if *ty == Type::JSON || *ty == Type::JSONB {
            return self.0.to_sql(ty, out);
        }
        if *ty == Type::BOOL {
            return json_bool(&self.0).to_sql(ty, out);
        }
        if *ty == Type::INT2 || *ty == Type::INT4 || *ty == Type::INT8 || *ty == Type::OID {
            let Some(number) = json_i64(&self.0) else {
                return Err(format!("Cannot bind {} as {ty}.", json_text(&self.0)).into());
            };
            return encode_integer(number, ty, out);
        }
        if *ty == Type::FLOAT4 || *ty == Type::FLOAT8 {
            let Some(number) = json_f64(&self.0) else {
                return Err(format!("Cannot bind {} as {ty}.", json_text(&self.0)).into());
            };
            return if *ty == Type::FLOAT4 {
                (number as f32).to_sql(ty, out)
            } else {
                number.to_sql(ty, out)
            };
        }
        if *ty == Type::NUMERIC {
            encode_numeric(&json_text(&self.0), out)?;
            return Ok(IsNull::No);
        }
        if *ty == Type::UUID {
            let id = Uuid::parse_str(json_text(&self.0).trim())?;
            return id.to_sql(ty, out);
        }
        if *ty == Type::DATE || *ty == Type::TIMESTAMP || *ty == Type::TIMESTAMPTZ {
            return encode_time(&json_text(&self.0), ty, out);
        }
        if is_text_type(ty) {
            return json_text(&self.0).to_sql(ty, out);
        }
        Err(format!("Cannot bind a value as {ty}.").into())
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }

    tokio_postgres::types::to_sql_checked!();
}

fn encode_integer(
    number: i64,
    ty: &Type,
    out: &mut BytesMut,
) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
    if *ty == Type::INT2 {
        let narrow = i16::try_from(number)?;
        return narrow.to_sql(ty, out);
    }
    if *ty == Type::INT4 {
        let narrow = i32::try_from(number)?;
        return narrow.to_sql(ty, out);
    }
    if *ty == Type::OID {
        let unsigned = u32::try_from(number)?;
        return unsigned.to_sql(ty, out);
    }
    number.to_sql(ty, out)
}

fn encode_time(
    text: &str,
    ty: &Type,
    out: &mut BytesMut,
) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
    let text = text.trim();
    if *ty == Type::DATE {
        let date = NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .or_else(|_| DateTime::parse_from_rfc3339(text).map(|value| value.date_naive()))?;
        return date.to_sql(ty, out);
    }
    if *ty == Type::TIMESTAMPTZ {
        let parsed = DateTime::parse_from_rfc3339(text)
            .map(|value| value.with_timezone(&Utc))
            .or_else(|_| {
                DateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f%#z")
                    .map(|value| value.with_timezone(&Utc))
            })?;
        return parsed.to_sql(ty, out);
    }
    let parsed = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| DateTime::parse_from_rfc3339(text).map(|value| value.naive_utc()))?;
    parsed.to_sql(ty, out)
}

fn is_text_type(ty: &Type) -> bool {
    *ty == Type::TEXT
        || *ty == Type::VARCHAR
        || *ty == Type::BPCHAR
        || *ty == Type::NAME
        || *ty == Type::UNKNOWN
        || ty.name() == "citext"
}

fn json_text(value: &JsonValue) -> String {
    match value {
        JsonValue::String(text) => text.clone(),
        JsonValue::Number(number) => number.to_string(),
        JsonValue::Bool(flag) => {
            if *flag {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        JsonValue::Null => String::new(),
        other => other.to_string(),
    }
}

fn json_bool(value: &JsonValue) -> bool {
    match value {
        JsonValue::Bool(flag) => *flag,
        JsonValue::Number(number) => number.as_i64() != Some(0),
        JsonValue::String(text) => {
            matches!(
                text.trim().to_ascii_lowercase().as_str(),
                "1" | "t" | "true"
            )
        }
        _ => false,
    }
}

fn json_i64(value: &JsonValue) -> Option<i64> {
    match value {
        JsonValue::Number(number) => number.as_i64().or_else(|| {
            number
                .as_u64()
                .and_then(|unsigned| i64::try_from(unsigned).ok())
        }),
        JsonValue::String(text) => text.trim().parse().ok(),
        JsonValue::Bool(flag) => Some(if *flag { 1 } else { 0 }),
        _ => None,
    }
}

fn json_f64(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Number(number) => number.as_f64(),
        JsonValue::String(text) => text.trim().parse().ok(),
        JsonValue::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// Postgres binary numeric: base-10000 digits, big-endian.
fn encode_numeric(
    text: &str,
    out: &mut BytesMut,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("nan") {
        out.put_u16(0);
        out.put_i16(0);
        out.put_u16(0xC000);
        out.put_i16(0);
        return Ok(());
    }
    let negative = text.starts_with('-');
    let body = text
        .strip_prefix('+')
        .or_else(|| text.strip_prefix('-'))
        .unwrap_or(text);
    if body.is_empty() {
        return Err(format!("Cannot bind {text} as numeric.").into());
    }
    let (whole, frac) = body.split_once('.').unwrap_or((body, ""));
    if !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !frac.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!("Cannot bind {text} as numeric.").into());
    }
    let dscale = i16::try_from(frac.len())?;
    let whole_digits = whole.trim_start_matches('0');
    let whole_groups = nbase_groups(whole_digits, true)?;
    let frac_groups = nbase_groups(frac, false)?;
    let whole_len = whole_groups.len();
    let mut digits = whole_groups;
    digits.extend(frac_groups);
    let leading = digits.iter().take_while(|digit| **digit == 0).count();
    let digits: Vec<i16> = digits.into_iter().skip(leading).collect();
    let trailing = digits.iter().rev().take_while(|digit| **digit == 0).count();
    let end = digits.len().saturating_sub(trailing);
    let digits: Vec<i16> = digits.into_iter().take(end).collect();
    let weight = if digits.is_empty() {
        0
    } else {
        i16::try_from(i32::try_from(whole_len)? - 1 - i32::try_from(leading)?)?
    };
    let ndigits = i16::try_from(digits.len())?;
    out.put_i16(ndigits);
    out.put_i16(weight);
    out.put_u16(if negative && !digits.is_empty() {
        0x4000
    } else {
        0
    });
    out.put_i16(dscale);
    for digit in digits {
        out.put_i16(digit);
    }
    Ok(())
}

fn nbase_groups(
    digits: &str,
    pad_left: bool,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Sync + Send>> {
    if digits.is_empty() {
        return Ok(Vec::new());
    }
    let pad = (4 - (digits.len() % 4)) % 4;
    let padded = if pad_left {
        format!("{}{digits}", "0".repeat(pad))
    } else {
        format!("{digits}{}", "0".repeat(pad))
    };
    let mut groups = Vec::new();
    let bytes = padded.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        let end = offset + 4;
        let chunk = bytes.get(offset..end).ok_or_else(|| {
            Box::new(std::io::Error::other("numeric digit group was short"))
                as Box<dyn std::error::Error + Sync + Send>
        })?;
        let text = std::str::from_utf8(chunk)?;
        groups.push(text.parse::<i16>()?);
        offset = end;
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::encode_numeric;
    use bytes::BytesMut;

    #[test]
    fn numeric_one_uses_postgres_base_10000() {
        let mut out = BytesMut::new();
        if let Err(err) = encode_numeric("1", &mut out) {
            panic!("{err}");
        }
        assert_eq!(out.as_ref(), &[0, 1, 0, 0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn numeric_keeps_fractional_scale() {
        let mut out = BytesMut::new();
        if let Err(err) = encode_numeric("12345.67", &mut out) {
            panic!("{err}");
        }
        assert_eq!(
            out.as_ref(),
            &[0, 3, 0, 1, 0, 0, 0, 2, 0, 1, 0x09, 0x29, 0x1A, 0x2C]
        );
    }
}
