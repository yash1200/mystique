//! Databases, tables, columns, and foreign-key relationships.

use mystique_connector_api::{
    CatalogColumnRow, CatalogDatabase, CatalogObject, CatalogRelationship, ColumnInfo,
    ColumnReference, ConnectorError, ConnectorResult,
};
use mystique_sql::{quote_ident, SqlDialect};

use super::query_failed;
use super::SqliteSession;

pub(super) async fn describe_table(
    session: &SqliteSession,
    _database: Option<&str>,
    _schema: Option<&str>,
    table: &str,
) -> ConnectorResult<Vec<ColumnInfo>> {
    session.ensure_open()?;
    let table = table.to_string();
    let pool = session.pool();
    session
        .blocking(move || -> ConnectorResult<Vec<ColumnInfo>> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            let mut stmt = conn
                .prepare(&sqlite_pragma_invoke("table_info", &table)?)
                .map_err(query_failed)?;
            let mut rows = stmt.query([]).map_err(query_failed)?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().map_err(query_failed)? {
                // cid, name, type, notnull, dflt_value, pk
                let name: String = row.get(1).map_err(query_failed)?;
                let ty: String = row
                    .get::<_, Option<String>>(2)
                    .map_err(query_failed)?
                    .unwrap_or_else(|| "ANY".into());
                let notnull: i64 = row.get(3).map_err(query_failed)?;
                let dflt: Option<String> = row.get(4).map_err(query_failed)?;
                let pk: i64 = row.get(5).map_err(query_failed)?;
                out.push(ColumnInfo {
                    name,
                    data_type: if ty.is_empty() { "ANY".into() } else { ty },
                    nullable: notnull == 0,
                    pk: pk != 0,
                    enum_values: None,
                    default_value: dflt.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                    references: None,
                });
            }

            // PRAGMA foreign_key_list: id, seq, table, from, to, on_update, on_delete, match
            if let Ok(mut fk_stmt) =
                conn.prepare(&sqlite_pragma_invoke("foreign_key_list", &table)?)
            {
                use std::collections::HashMap;
                let mut by_col: HashMap<String, ColumnReference> = HashMap::new();
                if let Ok(mut rows) = fk_stmt.query([]) {
                    while let Ok(Some(row)) = rows.next() {
                        let to_table: String = row.get(2).unwrap_or_default();
                        let from_col: String = row.get(3).unwrap_or_default();
                        let to_col: String = row.get(4).unwrap_or_default();
                        if from_col.is_empty() || to_table.is_empty() || to_col.is_empty() {
                            continue;
                        }
                        by_col.entry(from_col).or_insert(ColumnReference {
                            database: None,
                            schema: None,
                            table: to_table,
                            column: to_col,
                        });
                    }
                }
                for col in &mut out {
                    if let Some(r) = by_col.remove(&col.name) {
                        col.references = Some(r);
                    }
                }
            }

            Ok(out)
        })
        .await
}

pub(super) async fn list_catalog_columns(
    session: &SqliteSession,
    _database: Option<&str>,
    _schema: Option<&str>,
) -> ConnectorResult<Vec<CatalogColumnRow>> {
    session.ensure_open()?;
    let pool = session.pool();
    session
        .blocking(move || -> ConnectorResult<Vec<CatalogColumnRow>> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            let mut tables_stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master \
                 WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' \
                 ORDER BY name",
                )
                .map_err(query_failed)?;
            let tables: Vec<String> = tables_stmt
                .query_map([], |row| row.get(0))
                .map_err(query_failed)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(query_failed)?;

            let mut out = Vec::new();
            for table in tables {
                let mut stmt = conn
                    .prepare(&sqlite_pragma_invoke("table_info", &table)?)
                    .map_err(query_failed)?;
                let mut rows = stmt.query([]).map_err(query_failed)?;
                while let Some(row) = rows.next().map_err(query_failed)? {
                    let name: String = row.get(1).map_err(query_failed)?;
                    let ty: String = row
                        .get::<_, Option<String>>(2)
                        .map_err(query_failed)?
                        .unwrap_or_else(|| "ANY".into());
                    let notnull: i64 = row.get(3).map_err(query_failed)?;
                    let pk: i64 = row.get(5).map_err(query_failed)?;
                    out.push(CatalogColumnRow {
                        database: None,
                        schema: Some("main".into()),
                        table: table.clone(),
                        column: name,
                        data_type: if ty.is_empty() { "ANY".into() } else { ty },
                        nullable: notnull == 0,
                        pk: pk != 0,
                    });
                }
            }
            Ok(out)
        })
        .await
}

pub(super) async fn list_catalog(
    session: &SqliteSession,
    _preferred_database: Option<&str>,
    include_objects: bool,
) -> ConnectorResult<Vec<CatalogDatabase>> {
    session.ensure_open()?;
    let pool = session.pool();
    session
        .blocking(move || -> ConnectorResult<Vec<CatalogDatabase>> {
            let mut objects = Vec::new();
            if include_objects {
                let conn = pool
                    .get()
                    .map_err(|e| ConnectorError::Pool(e.to_string()))?;
                let mut stmt = conn
                    .prepare(
                        "SELECT name, type FROM sqlite_master \
                     WHERE type IN ('table','view','trigger') AND name NOT LIKE 'sqlite_%' \
                     ORDER BY name",
                    )
                    .map_err(query_failed)?;
                let mut rows = stmt.query([]).map_err(query_failed)?;
                while let Some(row) = rows.next().map_err(query_failed)? {
                    let name: String = row.get(0).map_err(query_failed)?;
                    let raw_type: String = row.get(1).map_err(query_failed)?;
                    let kind = match raw_type.to_ascii_lowercase().as_str() {
                        "view" => "view",
                        "trigger" => "trigger",
                        _ => "table",
                    };
                    objects.push(CatalogObject {
                        name,
                        schema: None,
                        database: Some("main".into()),
                        kind: kind.into(),
                        row_count: 0,
                    });
                }
            }
            Ok(vec![CatalogDatabase {
                name: "main".into(),
                meta: Some("live".into()),
                system: false,
                objects,
            }])
        })
        .await
}

pub(super) async fn list_relationships(
    session: &SqliteSession,
    _database: Option<&str>,
) -> ConnectorResult<Vec<CatalogRelationship>> {
    session.ensure_open()?;
    let pool = session.pool();
    session
        .blocking(move || -> ConnectorResult<Vec<CatalogRelationship>> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            let mut tables_stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master \
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
                 ORDER BY name",
                )
                .map_err(query_failed)?;
            let tables: Vec<String> = tables_stmt
                .query_map([], |row| row.get(0))
                .map_err(query_failed)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(query_failed)?;

            let mut out = Vec::new();
            for table in tables {
                let mut fk_stmt = conn
                    .prepare(&sqlite_pragma_invoke("foreign_key_list", &table)?)
                    .map_err(query_failed)?;
                let mut rows = fk_stmt.query([]).map_err(query_failed)?;
                while let Some(row) = rows.next().map_err(query_failed)? {
                    // id, seq, table, from, to, on_update, on_delete, match
                    let to_table: String = row.get(2).map_err(query_failed)?;
                    let from_col: String = row.get(3).map_err(query_failed)?;
                    let to_col: String = row.get(4).map_err(query_failed)?;
                    out.push(CatalogRelationship {
                        from_schema: None,
                        from_table: table.clone(),
                        from_column: from_col,
                        to_schema: None,
                        to_table,
                        to_column: to_col,
                        name: None,
                    });
                }
            }
            Ok(out)
        })
        .await
}

/// `PRAGMA name(ident)`. The argument is an identifier, not a quoted string,
/// so a value like `x'; DROP TABLE users` cannot close the statement.
pub(super) fn sqlite_pragma_invoke(pragma: &str, name: &str) -> ConnectorResult<String> {
    if name.chars().any(|c| c == '\0' || c.is_control()) {
        return Err(ConnectorError::Query("invalid object name".into()));
    }
    Ok(format!(
        "PRAGMA {pragma}({})",
        quote_ident(SqlDialect::Sqlite, name)
    ))
}

#[cfg(test)]
mod tests {
    use super::sqlite_pragma_invoke;

    #[test]
    fn pragma_argument_cannot_become_a_second_statement() {
        let payload = "x'; DROP TABLE users; -- ";
        let sql = sqlite_pragma_invoke("table_info", payload).unwrap();
        assert_eq!(sql, "PRAGMA table_info(\"x'; DROP TABLE users; -- \")");
        assert!(sqlite_pragma_invoke("table_info", "a\0b").is_err());
    }
}
