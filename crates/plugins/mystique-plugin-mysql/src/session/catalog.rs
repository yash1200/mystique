//! Databases, tables, columns, and foreign-key relationships.

use mysql_async::prelude::*;
use mystique_connector_api::{
    CatalogColumnRow, CatalogDatabase, CatalogObject, CatalogRelationship, ColumnInfo,
    ColumnReference, ConnectorError, ConnectorResult,
};

use super::encode::{mysql_default_display, parse_mysql_enum_values};
use super::query_failed;
use super::MysqlSession;

type RelationshipRow = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
);

pub(super) async fn describe_table(
    session: &MysqlSession,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> ConnectorResult<Vec<ColumnInfo>> {
    session.ensure_open()?;
    let db_name = database.or(schema).ok_or_else(|| {
        ConnectorError::InvalidConfig("MySQL describe_table requires database/schema".into())
    })?;
    let db = db_name.replace('\'', "''");
    let tbl = table.replace('\'', "''");
    let mut conn = session.conn().await?;

    // column_type is already DDL-style (varchar(255), datetime, timestamp, …)
    let sql = format!(
        "SELECT column_name, column_type, is_nullable, column_key, column_default, extra \
         FROM information_schema.columns \
         WHERE table_schema = '{db}' AND table_name = '{tbl}' \
         ORDER BY ordinal_position"
    );
    let rows: Vec<(String, String, String, String, Option<String>, String)> =
        conn.query(sql).await.map_err(query_failed)?;

    let mut out = Vec::with_capacity(rows.len());
    for (name, column_type, is_nullable, column_key, column_default, extra) in rows {
        let enum_values = parse_mysql_enum_values(&column_type);
        let default_value = mysql_default_display(column_default.as_deref(), &extra);
        out.push(ColumnInfo {
            name,
            data_type: column_type,
            nullable: !is_nullable.eq_ignore_ascii_case("NO"),
            pk: column_key.eq_ignore_ascii_case("PRI"),
            enum_values,
            default_value,
            references: None,
        });
    }

    // Attach FK targets for grid jump-to-row navigation.
    let fks: Vec<(String, Option<String>, String, String)> = conn
        .exec_map(
            r#"
            SELECT
              COLUMN_NAME,
              REFERENCED_TABLE_SCHEMA,
              REFERENCED_TABLE_NAME,
              REFERENCED_COLUMN_NAME
            FROM information_schema.KEY_COLUMN_USAGE
            WHERE TABLE_SCHEMA = ?
              AND TABLE_NAME = ?
              AND REFERENCED_TABLE_NAME IS NOT NULL
            ORDER BY CONSTRAINT_NAME, ORDINAL_POSITION
            "#,
            (db_name, table),
            |(col, rschema, rtable, rcol): (String, Option<String>, String, String)| {
                (col, rschema, rtable, rcol)
            },
        )
        .await
        .unwrap_or_default();
    use std::collections::HashMap;
    let mut by_col: HashMap<String, ColumnReference> = HashMap::new();
    for (col, rschema, rtable, rcol) in fks {
        by_col.entry(col).or_insert(ColumnReference {
            database: rschema.or_else(|| Some(db_name.to_string())),
            schema: None,
            table: rtable,
            column: rcol,
        });
    }
    for col in &mut out {
        if let Some(r) = by_col.remove(&col.name) {
            col.references = Some(r);
        }
    }

    Ok(out)
}

pub(super) async fn list_catalog_columns(
    session: &MysqlSession,
    database: Option<&str>,
    schema: Option<&str>,
) -> ConnectorResult<Vec<CatalogColumnRow>> {
    session.ensure_open()?;
    let mut conn = session.conn().await?;

    let sql = if let Some(db) = database.or(schema) {
        let db = db.replace('\'', "''");
        format!(
            "SELECT table_schema, table_name, column_name, column_type, is_nullable, column_key \
             FROM information_schema.columns \
             WHERE table_schema = '{db}' \
             ORDER BY table_schema, table_name, ordinal_position"
        )
    } else {
        "SELECT table_schema, table_name, column_name, column_type, is_nullable, column_key \
         FROM information_schema.columns \
         WHERE table_schema NOT IN ('mysql','information_schema','performance_schema','sys') \
         ORDER BY table_schema, table_name, ordinal_position"
            .to_string()
    };

    let rows: Vec<(String, String, String, String, String, String)> =
        conn.query(sql).await.map_err(query_failed)?;

    Ok(rows
        .into_iter()
        .map(
            |(table_schema, table_name, column_name, column_type, is_nullable, column_key)| {
                CatalogColumnRow {
                    database: Some(table_schema.clone()),
                    schema: Some(table_schema),
                    table: table_name,
                    column: column_name,
                    data_type: column_type,
                    nullable: !is_nullable.eq_ignore_ascii_case("NO"),
                    pk: column_key.eq_ignore_ascii_case("PRI"),
                }
            },
        )
        .collect())
}

pub(super) async fn list_catalog(
    session: &MysqlSession,
    preferred_database: Option<&str>,
    include_objects: bool,
) -> ConnectorResult<Vec<CatalogDatabase>> {
    session.ensure_open()?;
    let mut conn = session.conn().await?;
    let mut by_db: std::collections::BTreeMap<String, Vec<CatalogObject>> =
        std::collections::BTreeMap::new();
    // Include empty databases (no tables yet) so the picker can switch to them.
    if let Ok(schemas) = conn
        .query::<String, _>(
            "SELECT schema_name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('mysql','information_schema','performance_schema','sys') \
             ORDER BY schema_name",
        )
        .await
    {
        for name in schemas {
            by_db.entry(name).or_default();
        }
    }
    let rows: Vec<(String, String, String, i64)> = if include_objects {
        let sql =
            "SELECT table_schema, table_name, table_type, COALESCE(table_rows, 0) AS table_rows \
             FROM information_schema.tables \
             WHERE table_schema NOT IN ('mysql','information_schema','performance_schema','sys') \
             ORDER BY table_schema, table_name";
        conn.query(sql).await.map_err(query_failed)?
    } else {
        Vec::new()
    };
    for (schema, name, raw_type, approx) in rows {
        let kind = if raw_type.to_ascii_lowercase().contains("view") {
            "view"
        } else {
            "table"
        };
        by_db
            .entry(schema.clone())
            .or_default()
            .push(CatalogObject {
                name,
                schema: Some(schema.clone()),
                database: Some(schema),
                kind: kind.into(),
                row_count: approx.max(0) as u64,
            });
    }
    if let Some(pref) = preferred_database.map(str::trim).filter(|s| !s.is_empty()) {
        by_db.entry(pref.to_string()).or_default();
    }
    let pref = preferred_database.map(str::trim).unwrap_or("");
    let mut names: Vec<String> = by_db.keys().cloned().collect();
    names.sort_by(|a, b| {
        if !pref.is_empty() && a == pref {
            return std::cmp::Ordering::Less;
        }
        if !pref.is_empty() && b == pref {
            return std::cmp::Ordering::Greater;
        }
        if a == "mystique" {
            return std::cmp::Ordering::Less;
        }
        if b == "mystique" {
            return std::cmp::Ordering::Greater;
        }
        a.cmp(b)
    });
    Ok(names
        .into_iter()
        .map(|name| {
            let objects = by_db.remove(&name).unwrap_or_default();
            let meta = if objects.is_empty() {
                Some("empty".into())
            } else {
                Some(format!("{} tables", objects.len()))
            };
            CatalogDatabase {
                name,
                meta,
                system: false,
                objects,
            }
        })
        .collect())
}

pub(super) async fn list_relationships(
    session: &MysqlSession,
    database: Option<&str>,
) -> ConnectorResult<Vec<CatalogRelationship>> {
    session.ensure_open()?;
    let mut conn = session.conn().await?;
    let sql = if let Some(db) = database.map(str::trim).filter(|s| !s.is_empty()) {
        let db = db.replace('\'', "''");
        format!(
            "SELECT table_name, column_name, referenced_table_name, referenced_column_name, constraint_name \
             FROM information_schema.key_column_usage \
             WHERE table_schema = '{db}' AND referenced_table_name IS NOT NULL \
             ORDER BY table_name, ordinal_position"
        )
    } else {
        "SELECT table_schema, table_name, column_name, referenced_table_schema, referenced_table_name, referenced_column_name, constraint_name \
         FROM information_schema.key_column_usage \
         WHERE referenced_table_name IS NOT NULL \
           AND table_schema NOT IN ('mysql','information_schema','performance_schema','sys') \
         ORDER BY table_schema, table_name"
            .into()
    };
    if database.map(str::trim).filter(|s| !s.is_empty()).is_some() {
        let rows: Vec<(String, String, String, String, String)> =
            conn.query(sql).await.map_err(query_failed)?;
        let schema = database.unwrap().trim().to_string();
        return Ok(rows
            .into_iter()
            .map(
                |(from_table, from_column, to_table, to_column, name)| CatalogRelationship {
                    from_schema: Some(schema.clone()),
                    from_table,
                    from_column,
                    to_schema: Some(schema.clone()),
                    to_table,
                    to_column,
                    name: Some(name),
                },
            )
            .collect());
    }
    let rows: Vec<RelationshipRow> = conn.query(sql).await.map_err(query_failed)?;
    Ok(rows
        .into_iter()
        .map(
            |(from_schema, from_table, from_column, to_schema, to_table, to_column, name)| {
                CatalogRelationship {
                    from_schema: Some(from_schema),
                    from_table,
                    from_column,
                    to_schema,
                    to_table,
                    to_column,
                    name: Some(name),
                }
            },
        )
        .collect())
}
