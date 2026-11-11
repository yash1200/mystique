//! CREATE / rename / drop / truncate / duplicate.

use mystique_connector_api::{
    ConnectorError, ConnectorResult, TableAdminInput, TableAdminOp, TableAdminResult,
    TableObjectKind,
};
use mystique_sql::{qualify_table, quote_ident, SqlDialect};

use super::PostgresSession;
use super::{pg_col, pg_u32_col, query_failed};

pub(super) async fn apply_table_admin(
    session: &PostgresSession,
    input: &TableAdminInput,
) -> ConnectorResult<TableAdminResult> {
    session.ensure_open()?;
    let client = session.client().await?;
    let schema = input
        .schema
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("public");
    let qname = qualify_table(SqlDialect::Postgres, None, Some(schema), &input.name);
    let is_view = input.kind == TableObjectKind::View;

    match input.op {
        TableAdminOp::CreateStatement => {
            let sql = match input.object_kind.as_str() {
                "function" | "procedure" => {
                    routine_definition(&client, schema, &input.name).await?
                }
                "sequence" => sequence_definition(&client, schema, &input.name).await?,
                "type" => type_definition(&client, schema, &input.name).await?,
                _ => pg_create_statement(&client, schema, &input.name, is_view).await?,
            };
            Ok(TableAdminResult::statement(input.op, sql))
        }
        TableAdminOp::CreateGroup => {
            let qschema = quote_ident(SqlDialect::Postgres, input.name.trim());
            let sql = format!("CREATE SCHEMA {qschema}");
            client.batch_execute(&sql).await.map_err(query_failed)?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Created schema {qschema}."),
            ))
        }
        TableAdminOp::CreateObject => {
            // Minimal empty table with a PK so CREATE TABLE is valid.
            let sql = format!("CREATE TABLE {qname} (id bigserial PRIMARY KEY)");
            client.batch_execute(&sql).await.map_err(query_failed)?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Created table {qname}."),
            ))
        }
        TableAdminOp::Rename => {
            let new_name = input.new_name.as_deref().unwrap_or("").trim();
            let dest = quote_ident(SqlDialect::Postgres, new_name);
            let kw = if is_view { "VIEW" } else { "TABLE" };
            let sql = format!("ALTER {kw} {qname} RENAME TO {dest}");
            client.batch_execute(&sql).await.map_err(query_failed)?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Renamed {qname} to {dest}."),
            ))
        }
        TableAdminOp::Drop => {
            let kw = if is_view { "VIEW" } else { "TABLE" };
            let sql = format!("DROP {kw} IF EXISTS {qname}");
            client.batch_execute(&sql).await.map_err(query_failed)?;
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
            client.batch_execute(&sql).await.map_err(query_failed)?;
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
            let dest = qualify_table(SqlDialect::Postgres, None, Some(schema), new_name);
            let sql = format!(
                "CREATE TABLE {dest} (LIKE {qname} INCLUDING ALL); INSERT INTO {dest} SELECT * FROM {qname}"
            );
            client.batch_execute(&sql).await.map_err(query_failed)?;
            Ok(TableAdminResult::executed(
                input.op,
                format!("Duplicated {qname} to {dest}."),
            ))
        }
    }
}

async fn pg_create_statement(
    client: &deadpool_postgres::Object,
    schema: &str,
    name: &str,
    prefer_view: bool,
) -> ConnectorResult<String> {
    let found = client
        .query_opt(
            r#"
        SELECT c.oid, c.relkind::text
        FROM pg_catalog.pg_class c
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = $1 AND c.relname = $2
        "#,
            &[&schema, &name],
        )
        .await
        .map_err(query_failed)?;
    let Some(row) = found else {
        return Err(ConnectorError::Query(format!(
            "Object `{schema}.{name}` was not found."
        )));
    };
    let oid = pg_u32_col(&row, 0)?;
    let relkind: String = pg_col(&row, 1)?;
    let is_view = prefer_view || relkind == "v" || relkind == "m";
    let qname = qualify_table(SqlDialect::Postgres, None, Some(schema), name);

    if is_view {
        let def_row = client
            .query_one("SELECT pg_get_viewdef($1::oid, true)", &[&oid])
            .await
            .map_err(query_failed)?;
        let def: String = pg_col(&def_row, 0)?;
        let kw = if relkind == "m" {
            "MATERIALIZED VIEW"
        } else {
            "VIEW"
        };
        return Ok(format!("CREATE {kw} {qname} AS\n{def}\n"));
    }

    let cols = client
        .query(
            r#"
        SELECT a.attname,
               pg_catalog.format_type(a.atttypid, a.atttypmod),
               a.attnotnull
        FROM pg_catalog.pg_attribute a
        WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped
        ORDER BY a.attnum
        "#,
            &[&oid],
        )
        .await
        .map_err(query_failed)?;
    let mut lines = Vec::with_capacity(cols.len());
    for c in &cols {
        let col: String = pg_col(c, 0)?;
        let typ: String = pg_col(c, 1)?;
        let not_null: bool = pg_col(c, 2)?;
        let nn = if not_null { " NOT NULL" } else { "" };
        lines.push(format!(
            "  {} {typ}{nn}",
            quote_ident(SqlDialect::Postgres, &col)
        ));
    }
    let pk = client
    .query_opt(
        "SELECT pg_get_constraintdef(oid) FROM pg_catalog.pg_constraint WHERE conrelid = $1 AND contype = 'p'",
        &[&oid],
    )
    .await
    .map_err(query_failed)?;
    if let Some(row) = pk {
        let def: String = pg_col(&row, 0)?;
        lines.push(format!("  {def}"));
    }
    if lines.is_empty() {
        lines.push("  -- (no columns)".into());
    }
    Ok(format!(
        "CREATE TABLE {qname} (\n{}\n);\n",
        lines.join(",\n")
    ))
}

/// `CREATE OR REPLACE FUNCTION …` for a routine listed as `name(identity args)`.
async fn routine_definition(
    client: &deadpool_postgres::Object,
    schema: &str,
    listed: &str,
) -> ConnectorResult<String> {
    let row = client
        .query_opt(
            r#"
        SELECT pg_catalog.pg_get_functiondef(p.oid)
        FROM pg_catalog.pg_proc p
        JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
        WHERE n.nspname = $1
          AND p.proname || '(' || pg_catalog.pg_get_function_identity_arguments(p.oid) || ')' = $2
        "#,
            &[&schema, &listed],
        )
        .await
        .map_err(query_failed)?;
    let Some(row) = row else {
        return Err(not_found(schema, listed));
    };
    let def: String = pg_col(&row, 0)?;
    Ok(format!("{};\n", def.trim_end()))
}

async fn sequence_definition(
    client: &deadpool_postgres::Object,
    schema: &str,
    name: &str,
) -> ConnectorResult<String> {
    let row = client
        .query_opt(
            r#"
        SELECT pg_catalog.format_type(s.seqtypid, NULL),
               s.seqincrement, s.seqmin, s.seqmax, s.seqstart, s.seqcache, s.seqcycle
        FROM pg_catalog.pg_sequence s
        JOIN pg_catalog.pg_class c ON c.oid = s.seqrelid
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = $1 AND c.relname = $2
        "#,
            &[&schema, &name],
        )
        .await
        .map_err(query_failed)?;
    let Some(row) = row else {
        return Err(not_found(schema, name));
    };
    let typ: String = pg_col(&row, 0)?;
    let increment: i64 = pg_col(&row, 1)?;
    let min: i64 = pg_col(&row, 2)?;
    let max: i64 = pg_col(&row, 3)?;
    let start: i64 = pg_col(&row, 4)?;
    let cache: i64 = pg_col(&row, 5)?;
    let cycle: bool = pg_col(&row, 6)?;
    let qname = qualify_table(SqlDialect::Postgres, None, Some(schema), name);
    let cycle = if cycle { "CYCLE" } else { "NO CYCLE" };
    Ok(format!(
        "CREATE SEQUENCE {qname}\n  AS {typ}\n  INCREMENT BY {increment}\n  MINVALUE {min}\n  MAXVALUE {max}\n  START WITH {start}\n  CACHE {cache}\n  {cycle};\n"
    ))
}

/// Enum, composite or domain.
async fn type_definition(
    client: &deadpool_postgres::Object,
    schema: &str,
    name: &str,
) -> ConnectorResult<String> {
    let row = client
        .query_opt(
            r#"
        SELECT t.oid, t.typtype::text, t.typrelid,
               pg_catalog.format_type(t.typbasetype, t.typtypmod),
               t.typnotnull, t.typdefault
        FROM pg_catalog.pg_type t
        JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
        WHERE n.nspname = $1 AND t.typname = $2
        "#,
            &[&schema, &name],
        )
        .await
        .map_err(query_failed)?;
    let Some(row) = row else {
        return Err(not_found(schema, name));
    };
    let oid = pg_u32_col(&row, 0)?;
    let typtype: String = pg_col(&row, 1)?;
    let relid = pg_u32_col(&row, 2)?;
    let qname = qualify_table(SqlDialect::Postgres, None, Some(schema), name);
    match typtype.as_str() {
        "e" => {
            let labels = client
                .query(
                    "SELECT enumlabel FROM pg_catalog.pg_enum WHERE enumtypid = $1 ORDER BY enumsortorder",
                    &[&oid],
                )
                .await
                .map_err(query_failed)?;
            let mut out = Vec::with_capacity(labels.len());
            for l in &labels {
                let label: String = pg_col(l, 0)?;
                out.push(format!("  '{}'", label.replace('\'', "''")));
            }
            Ok(format!(
                "CREATE TYPE {qname} AS ENUM (\n{}\n);\n",
                out.join(",\n")
            ))
        }
        "c" => {
            let attrs = client
                .query(
                    r#"
                SELECT a.attname, pg_catalog.format_type(a.atttypid, a.atttypmod)
                FROM pg_catalog.pg_attribute a
                WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped
                ORDER BY a.attnum
                "#,
                    &[&relid],
                )
                .await
                .map_err(query_failed)?;
            let mut out = Vec::with_capacity(attrs.len());
            for a in &attrs {
                let col: String = pg_col(a, 0)?;
                let typ: String = pg_col(a, 1)?;
                out.push(format!(
                    "  {} {typ}",
                    quote_ident(SqlDialect::Postgres, &col)
                ));
            }
            Ok(format!(
                "CREATE TYPE {qname} AS (\n{}\n);\n",
                out.join(",\n")
            ))
        }
        "d" => {
            let base: String = pg_col(&row, 3)?;
            let not_null: bool = pg_col(&row, 4)?;
            let default: Option<String> = pg_col(&row, 5)?;
            let checks = client
                .query(
                    "SELECT pg_catalog.pg_get_constraintdef(oid) FROM pg_catalog.pg_constraint WHERE contypid = $1 ORDER BY conname",
                    &[&oid],
                )
                .await
                .map_err(query_failed)?;
            let mut sql = format!("CREATE DOMAIN {qname} AS {base}");
            if let Some(d) = default {
                sql.push_str(&format!("\n  DEFAULT {d}"));
            }
            if not_null {
                sql.push_str("\n  NOT NULL");
            }
            for c in &checks {
                let def: String = pg_col(c, 0)?;
                sql.push_str(&format!("\n  {def}"));
            }
            sql.push_str(";\n");
            Ok(sql)
        }
        other => Err(ConnectorError::NotSupported(format!(
            "No definition for a type of kind `{other}`."
        ))),
    }
}

fn not_found(schema: &str, name: &str) -> ConnectorError {
    ConnectorError::Query(format!("Object `{schema}.{name}` was not found."))
}
