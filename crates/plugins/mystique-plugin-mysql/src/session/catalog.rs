//! Databases, tables, columns, and foreign-key relationships.

use mystique_connector_api::{
    CatalogColumnRow, CatalogDatabase, CatalogObject, CatalogRelationship, ColumnInfo,
    ColumnReference, ConnectorError, ConnectorResult,
};

use super::encode::{mysql_default_display, parse_mysql_enum_values};
use super::MysqlSession;

type RelationshipRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

/// One schema: from table, from column, to table, to column, constraint.
type SchemaRelationshipRow = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

/// Column, referenced schema, referenced table, referenced column.
type ColumnReferenceRow = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

const DESCRIBE_COLUMNS_SQL: &str = "\
SELECT column_name, column_type, is_nullable, column_key, column_default, extra \
FROM information_schema.columns \
WHERE table_schema = ? AND table_name = ? \
ORDER BY ordinal_position";

const LIST_COLUMNS_ONE_SCHEMA_SQL: &str = "\
SELECT table_schema, table_name, column_name, column_type, is_nullable, column_key \
FROM information_schema.columns \
WHERE table_schema = ? \
ORDER BY table_schema, table_name, ordinal_position";

const LIST_COLUMNS_ALL_SCHEMAS_SQL: &str = "\
SELECT table_schema, table_name, column_name, column_type, is_nullable, column_key \
FROM information_schema.columns \
WHERE table_schema NOT IN ('mysql','information_schema','performance_schema','sys') \
ORDER BY table_schema, table_name, ordinal_position";

const LIST_RELATIONSHIPS_ONE_SCHEMA_SQL: &str = "\
SELECT table_name, column_name, referenced_table_name, referenced_column_name, constraint_name \
FROM information_schema.key_column_usage \
WHERE table_schema = ? AND referenced_table_name IS NOT NULL \
ORDER BY table_name, ordinal_position";

const LIST_RELATIONSHIPS_ALL_SCHEMAS_SQL: &str = "\
SELECT table_schema, table_name, column_name, referenced_table_schema, referenced_table_name, referenced_column_name, constraint_name \
FROM information_schema.key_column_usage \
WHERE referenced_table_name IS NOT NULL \
  AND table_schema NOT IN ('mysql','information_schema','performance_schema','sys') \
ORDER BY table_schema, table_name";

pub(super) async fn describe_columns(
    session: &MysqlSession,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> ConnectorResult<Vec<ColumnInfo>> {
    session.ensure_open()?;
    let db_name = database.or(schema).ok_or_else(|| {
        ConnectorError::InvalidConfig("MySQL describe_table requires database/schema".into())
    })?;
    let mut conn = session.conn().await?;

    // column_type is already DDL-style (varchar(255), datetime, timestamp, …)
    // Names are bound. Concatenating them reopens SEC-04 (`\'` is a quote on MySQL).
    let rows: Vec<(String, String, String, String, Option<String>, String)> =
        crate::rows::collect_exec(&mut conn, DESCRIBE_COLUMNS_SQL, (db_name, table)).await?;

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

    Ok(out)
}

pub(super) async fn table_references(
    session: &MysqlSession,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> ConnectorResult<Vec<(String, ColumnReference)>> {
    session.ensure_open()?;
    let db_name = database.or(schema).ok_or_else(|| {
        ConnectorError::InvalidConfig("MySQL table_references requires database/schema".into())
    })?;
    let mut conn = session.conn().await?;
    let fks: Vec<ColumnReferenceRow> = crate::rows::collect_exec(
        &mut conn,
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
    )
    .await
    .unwrap_or_default();
    use std::collections::HashMap;
    let mut by_col: HashMap<String, ColumnReference> = HashMap::new();
    for (col, rschema, rtable, rcol) in fks {
        let Some(col) = col.filter(|name| !name.is_empty()) else {
            continue;
        };
        let Some(rtable) = rtable.filter(|name| !name.is_empty()) else {
            continue;
        };
        by_col.entry(col).or_insert(ColumnReference {
            database: rschema.or_else(|| Some(db_name.to_string())),
            schema: None,
            table: rtable,
            column: rcol.unwrap_or_default(),
        });
    }
    Ok(by_col.into_iter().collect())
}

pub(super) async fn describe_table(
    session: &MysqlSession,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> ConnectorResult<Vec<ColumnInfo>> {
    let mut out = describe_columns(session, database, schema, table).await?;
    if let Ok(refs) = table_references(session, database, schema, table).await {
        for col in &mut out {
            if let Some((_, r)) = refs.iter().find(|(name, _)| name == &col.name) {
                col.references = Some(r.clone());
            }
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

    let rows: Vec<(String, String, String, String, String, String)> =
        if let Some(db) = database.or(schema) {
            crate::rows::collect_exec(&mut conn, LIST_COLUMNS_ONE_SCHEMA_SQL, (db,)).await?
        } else {
            crate::rows::collect_query(&mut conn, LIST_COLUMNS_ALL_SCHEMAS_SQL).await?
        };

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

/// Functions, procedures and triggers as `(schema, name, kind)`.
const OTHER_OBJECTS_SQL: &str = "\
    SELECT routine_schema, routine_name, LOWER(routine_type) \
    FROM information_schema.routines \
    WHERE routine_schema NOT IN ('mysql','information_schema','performance_schema','sys') \
    UNION ALL \
    SELECT trigger_schema, trigger_name, 'trigger' \
    FROM information_schema.triggers \
    WHERE trigger_schema NOT IN ('mysql','information_schema','performance_schema','sys') \
    ORDER BY 1, 3, 2";

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
    if let Ok(schemas) = crate::rows::collect_query::<String>(
        &mut conn,
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
        crate::rows::collect_query(&mut conn, sql).await?
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
    if include_objects {
        // Routines and triggers. A server without the privilege to read
        // them still lists its tables.
        if let Ok(other) =
            crate::rows::collect_query::<(String, String, String)>(&mut conn, OTHER_OBJECTS_SQL)
                .await
        {
            for (schema, name, kind) in other {
                by_db
                    .entry(schema.clone())
                    .or_default()
                    .push(CatalogObject {
                        name,
                        schema: Some(schema.clone()),
                        database: Some(schema),
                        kind,
                        row_count: 0,
                    });
            }
        }
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
                let tables = objects
                    .iter()
                    .filter(|o| o.kind == "table" || o.kind == "view")
                    .count();
                Some(format!("{tables} tables"))
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
    if let Some(db) = database.map(str::trim).filter(|s| !s.is_empty()) {
        let rows: Vec<SchemaRelationshipRow> =
            crate::rows::collect_exec(&mut conn, LIST_RELATIONSHIPS_ONE_SCHEMA_SQL, (db,)).await?;
        let schema = db.to_string();
        return Ok(rows
            .into_iter()
            .map(
                |(from_table, from_column, to_table, to_column, name)| CatalogRelationship {
                    from_schema: Some(schema.clone()),
                    from_table,
                    from_column: from_column.unwrap_or_default(),
                    to_schema: Some(schema.clone()),
                    to_table: to_table.unwrap_or_default(),
                    to_column: to_column.unwrap_or_default(),
                    name: Some(name),
                },
            )
            .collect());
    }
    let rows: Vec<RelationshipRow> =
        crate::rows::collect_query(&mut conn, LIST_RELATIONSHIPS_ALL_SCHEMAS_SQL).await?;
    Ok(rows
        .into_iter()
        .map(
            |(from_schema, from_table, from_column, to_schema, to_table, to_column, name)| {
                CatalogRelationship {
                    from_schema: Some(from_schema),
                    from_table,
                    from_column: from_column.unwrap_or_default(),
                    to_schema,
                    to_table: to_table.unwrap_or_default(),
                    to_column: to_column.unwrap_or_default(),
                    name: Some(name),
                }
            },
        )
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_names_are_bound_not_concatenated() {
        let payload = "x\\'; DROP TABLE users; -- ";
        for sql in [
            DESCRIBE_COLUMNS_SQL,
            LIST_COLUMNS_ONE_SCHEMA_SQL,
            LIST_RELATIONSHIPS_ONE_SCHEMA_SQL,
        ] {
            assert!(sql.contains('?'), "{sql}");
            assert!(!sql.contains(payload), "{sql}");
            assert!(!sql.to_ascii_lowercase().contains("drop table"), "{sql}");
            assert!(!sql.contains("'{"), "{sql}");
        }
    }
}
