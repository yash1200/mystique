//! CREATE / rename / drop / truncate / duplicate.

use mysql_async::prelude::*;
use mystique_connector_api::{
    ConnectorError, ConnectorResult, TableAdminInput, TableAdminOp, TableAdminResult,
    TableObjectKind,
};
use mystique_sql::{qualify_table, SqlDialect};

use super::query_failed;
use super::MysqlSession;

pub(super) async fn apply_table_admin(
    session: &MysqlSession,
    input: &TableAdminInput,
) -> ConnectorResult<TableAdminResult> {
    session.ensure_open()?;
    let mut conn = session.conn().await?;
    let qname = qualify_table(
        SqlDialect::Mysql,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.name,
    );
    let is_view = input.kind == TableObjectKind::View;

    match input.op {
        TableAdminOp::CreateStatement => {
            // `SHOW CREATE FUNCTION` and friends put the text in column 2.
            let (kw, column) = match input.object_kind.as_str() {
                "function" => ("FUNCTION", 2),
                "procedure" => ("PROCEDURE", 2),
                "trigger" => ("TRIGGER", 2),
                _ if is_view => ("VIEW", 1),
                _ => ("TABLE", 1),
            };
            let sql = format!("SHOW CREATE {kw} {qname}");
            let row = crate::rows::query_first_opt::<mysql_async::Row>(&mut conn, &sql).await?;
            let row =
                row.ok_or_else(|| ConnectorError::Query(format!("Object {qname} was not found.")))?;
            let create = match row.get_opt::<Option<String>, _>(column) {
                Some(Ok(value)) => value,
                Some(Err(err)) => {
                    return Err(ConnectorError::Query(format!(
                        "Could not read CREATE text for {qname}: {err}"
                    )));
                }
                None => None,
            };
            let statement = create
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| ConnectorError::Query(format!("No CREATE text for {qname}.")))?;
            let statement = if statement.trim_end().ends_with(';') {
                format!("{}\n", statement.trim_end())
            } else {
                format!("{};\n", statement.trim_end())
            };
            Ok(TableAdminResult::statement(input.op, statement))
        }
        TableAdminOp::CreateGroup => Err(ConnectorError::NotSupported(
            "MySQL does not use schemas; create a database instead.".into(),
        )),
        TableAdminOp::CreateObject => {
            let sql =
                format!("CREATE TABLE {qname} (id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY)");
            conn.query_drop(sql).await.map_err(query_failed)?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Created table {qname}."),
            ))
        }
        TableAdminOp::Rename => {
            let new_name = input.new_name.as_deref().unwrap_or("").trim();
            let dest = qualify_table(
                SqlDialect::Mysql,
                input.database.as_deref(),
                input.schema.as_deref(),
                new_name,
            );
            let sql = format!("RENAME TABLE {qname} TO {dest}");
            conn.query_drop(sql).await.map_err(query_failed)?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Renamed {qname} to {dest}."),
            ))
        }
        TableAdminOp::Drop => {
            let kw = if is_view { "VIEW" } else { "TABLE" };
            let sql = format!("DROP {kw} IF EXISTS {qname}");
            conn.query_drop(sql).await.map_err(query_failed)?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Dropped {qname}."),
            ))
        }
        TableAdminOp::Truncate => {
            if is_view {
                return Err(ConnectorError::NotSupported(
                    "Cannot truncate a view.".into(),
                ));
            }
            let sql = format!("TRUNCATE TABLE {qname}");
            conn.query_drop(sql).await.map_err(query_failed)?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Truncated {qname}."),
            ))
        }
        TableAdminOp::Duplicate => {
            if is_view {
                return Err(ConnectorError::NotSupported(
                    "Duplicate is not supported for views.".into(),
                ));
            }
            let new_name = input.new_name.as_deref().unwrap_or("").trim();
            let dest = qualify_table(
                SqlDialect::Mysql,
                input.database.as_deref(),
                input.schema.as_deref(),
                new_name,
            );
            conn.query_drop(format!("CREATE TABLE {dest} LIKE {qname}"))
                .await
                .map_err(query_failed)?;
            conn.query_drop(format!("INSERT INTO {dest} SELECT * FROM {qname}"))
                .await
                .map_err(query_failed)?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Duplicated {qname} to {dest}."),
            ))
        }
    }
}
