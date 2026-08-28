//! CREATE / rename / drop / truncate / duplicate.

use mysql_async::prelude::*;
use mystique_connector_api::{
    qualify_table, ConnectorError, ConnectorResult, SqlIdentStyle, TableAdminInput, TableAdminOp,
    TableAdminResult, TableObjectKind,
};

use super::query_failed;
use super::MysqlSession;

pub(super) async fn apply_table_admin(
    session: &MysqlSession,
    input: &TableAdminInput,
) -> ConnectorResult<TableAdminResult> {
    session.ensure_open()?;
    let mut conn = session.conn().await?;
    let qname = qualify_table(
        SqlIdentStyle::Mysql,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.name,
    );
    let is_view = input.kind == TableObjectKind::View;

    match input.op {
        TableAdminOp::CreateStatement => {
            let kw = if is_view { "VIEW" } else { "TABLE" };
            let sql = format!("SHOW CREATE {kw} {qname}");
            let row: Option<mysql_async::Row> =
                conn.query_first(sql).await.map_err(query_failed)?;
            let row =
                row.ok_or_else(|| ConnectorError::Query(format!("Object {qname} was not found.")))?;
            let create: Option<String> = row.get(1);
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
                SqlIdentStyle::Mysql,
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
                SqlIdentStyle::Mysql,
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
