//! CREATE / rename / drop / truncate / duplicate.

use mystique_connector_api::{
    ConnectorError, ConnectorResult, TableAdminInput, TableAdminOp, TableAdminResult,
    TableObjectKind,
};
use mystique_sql::{quote_ident, SqlDialect};
use rusqlite::OptionalExtension;

use super::query_failed;
use super::SqliteSession;

pub(super) async fn apply_table_admin(
    session: &SqliteSession,
    input: &TableAdminInput,
) -> ConnectorResult<TableAdminResult> {
    session.ensure_open()?;
    let pool = session.pool();
    let op = input.op;
    let name = input.name.clone();
    let new_name = input.new_name.clone();
    let is_view = input.kind == TableObjectKind::View;
    session
        .blocking(move || -> ConnectorResult<TableAdminResult> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            let qname = quote_ident(SqlDialect::Sqlite, &name);
            match op {
                TableAdminOp::CreateStatement => {
                    let typ = if is_view { "view" } else { "table" };
                    let sql: Option<String> = conn
                        .query_row(
                            "SELECT sql FROM sqlite_master WHERE type = ?1 AND name = ?2",
                            rusqlite::params![typ, name],
                            |row| row.get(0),
                        )
                        .optional()
                        .map_err(query_failed)?;
                    let statement = sql.filter(|s| !s.trim().is_empty()).ok_or_else(|| {
                        ConnectorError::Query(format!("No CREATE text for {qname}."))
                    })?;
                    let statement = if statement.trim_end().ends_with(';') {
                        format!("{}\n", statement.trim_end())
                    } else {
                        format!("{};\n", statement.trim_end())
                    };
                    Ok(TableAdminResult::statement(op, statement))
                }
                TableAdminOp::CreateGroup => Err(ConnectorError::NotSupported(
                    "SQLite does not support creating schemas.".into(),
                )),
                TableAdminOp::CreateObject => {
                    conn.execute(
                        &format!("CREATE TABLE {qname} (id INTEGER PRIMARY KEY)"),
                        [],
                    )
                    .map_err(query_failed)?;
                    Ok(TableAdminResult::executed(
                        op,
                        format!("Created table {qname}."),
                    ))
                }
                TableAdminOp::Rename => {
                    let dest =
                        quote_ident(SqlDialect::Sqlite, new_name.as_deref().unwrap_or("").trim());
                    conn.execute(&format!("ALTER TABLE {qname} RENAME TO {dest}"), [])
                        .map_err(query_failed)?;
                    Ok(TableAdminResult::executed(
                        op,
                        format!("Renamed {qname} to {dest}."),
                    ))
                }
                TableAdminOp::Drop => {
                    let kw = if is_view { "VIEW" } else { "TABLE" };
                    conn.execute(&format!("DROP {kw} IF EXISTS {qname}"), [])
                        .map_err(query_failed)?;
                    Ok(TableAdminResult::executed(op, format!("Dropped {qname}.")))
                }
                TableAdminOp::Truncate => {
                    if is_view {
                        return Err(ConnectorError::NotSupported(
                            "Cannot truncate a view.".into(),
                        ));
                    }
                    conn.execute(&format!("DELETE FROM {qname}"), [])
                        .map_err(query_failed)?;
                    Ok(TableAdminResult::executed(
                        op,
                        format!("Truncated {qname}."),
                    ))
                }
                TableAdminOp::Duplicate => {
                    if is_view {
                        return Err(ConnectorError::NotSupported(
                            "Duplicate is not supported for views.".into(),
                        ));
                    }
                    let dest =
                        quote_ident(SqlDialect::Sqlite, new_name.as_deref().unwrap_or("").trim());
                    conn.execute(&format!("CREATE TABLE {dest} AS SELECT * FROM {qname}"), [])
                        .map_err(query_failed)?;
                    Ok(TableAdminResult::executed(
                        op,
                        format!("Duplicated {qname} to {dest}."),
                    ))
                }
            }
        })
        .await
}
