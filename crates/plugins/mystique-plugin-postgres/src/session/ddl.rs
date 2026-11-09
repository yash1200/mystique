//! Read table definitions and run a DDL plan on one connection.

use std::collections::BTreeMap;

use mystique_connector_api::{
    ApplyDdlResult, ColumnDraft, ConnectorError, ConnectorResult, ConstraintDraft, DdlPlan,
    ForeignKeyDraft, IndexDraft, TableDefinition,
};

use super::PostgresSession;
use super::{pg_col, pg_i32_col, query_failed};

pub(super) async fn describe_table_definition(
    session: &PostgresSession,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> ConnectorResult<TableDefinition> {
    let schema = nonempty(schema).unwrap_or("public");
    let mut tables = load(session, database, schema, Some(table)).await?;
    match tables.pop() {
        Some(definition) => Ok(definition),
        None => Err(ConnectorError::Query(format!(
            "Table `{schema}.{table}` was not found."
        ))),
    }
}

pub(super) async fn list_table_definitions(
    session: &PostgresSession,
    database: Option<&str>,
    schema: Option<&str>,
) -> ConnectorResult<Vec<TableDefinition>> {
    let schema = nonempty(schema).unwrap_or("public");
    load(session, database, schema, None).await
}

pub(super) async fn apply_ddl(
    session: &PostgresSession,
    plan: &DdlPlan,
    _draft: &TableDefinition,
) -> ConnectorResult<ApplyDdlResult> {
    session.ensure_open()?;
    if plan.statements.is_empty() {
        return Ok(empty_result());
    }
    let client = session.client().await?;
    let wrap = plan.transactional
        && !plan
            .statements
            .iter()
            .any(|sql| sql.trim().eq_ignore_ascii_case("BEGIN"));
    if wrap {
        client.batch_execute("BEGIN").await.map_err(query_failed)?;
    }
    let mut applied = Vec::new();
    for sql in &plan.statements {
        if let Err(err) = client.batch_execute(sql).await {
            if wrap {
                let _ = client.batch_execute("ROLLBACK").await;
            }
            return Err(partial_failure(applied.len(), &query_failed(err), wrap));
        }
        applied.push(sql.clone());
    }
    if wrap {
        if let Err(err) = client.batch_execute("COMMIT").await {
            let _ = client.batch_execute("ROLLBACK").await;
            return Err(partial_failure(applied.len(), &query_failed(err), true));
        }
    }
    Ok(done(applied))
}

async fn load(
    session: &PostgresSession,
    database: Option<&str>,
    schema: &str,
    table: Option<&str>,
) -> ConnectorResult<Vec<TableDefinition>> {
    session.ensure_open()?;
    let client = session.client().await?;
    let mut tables: BTreeMap<String, TableDefinition> = BTreeMap::new();
    let column_rows = match table {
        Some(table) => client
            .query(COLUMNS_ONE, &[&schema, &table])
            .await
            .map_err(query_failed)?,
        None => client
            .query(COLUMNS_ALL, &[&schema])
            .await
            .map_err(query_failed)?,
    };
    for row in &column_rows {
        let table_name: String = pg_col(row, 0)?;
        let column_name: String = pg_col(row, 1)?;
        let data_type: String = pg_col(row, 2)?;
        let nullable: bool = pg_col(row, 3)?;
        let pk = u32::try_from(pg_i32_col(row, 4)?.unwrap_or(0)).unwrap_or(0);
        let default: Option<String> = pg_col(row, 5)?;
        let identity: String = pg_col(row, 6)?;
        let definition = tables.entry(table_name.clone()).or_insert_with(|| {
            TableDefinition::loaded(
                database.map(str::to_string),
                Some(schema.to_string()),
                table_name,
                "table",
            )
        });
        let identity = match identity.as_str() {
            "a" => Some("always".to_string()),
            "d" => Some("by default".to_string()),
            _ => None,
        };
        definition.columns.push(
            ColumnDraft::loaded(column_name, data_type, nullable, pk)
                .with_default(default, identity),
        );
    }

    let index_rows = match table {
        Some(table) => client
            .query(INDEXES_ONE, &[&schema, &table])
            .await
            .map_err(query_failed)?,
        None => client
            .query(INDEXES_ALL, &[&schema])
            .await
            .map_err(query_failed)?,
    };
    for row in &index_rows {
        let table_name: String = pg_col(row, 0)?;
        let Some(definition) = tables.get_mut(&table_name) else {
            continue;
        };
        let index_name: String = pg_col(row, 1)?;
        let unique: bool = pg_col(row, 2)?;
        let primary: bool = pg_col(row, 3)?;
        let column_name: String = pg_col(row, 4)?;
        let index_sql: String = pg_col(row, 5)?;
        push_index(
            definition,
            index_name,
            unique,
            primary,
            column_name,
            index_sql,
        );
    }

    let fk_rows = match table {
        Some(table) => client
            .query(FKS_ONE, &[&schema, &table])
            .await
            .map_err(query_failed)?,
        None => client
            .query(FKS_ALL, &[&schema])
            .await
            .map_err(query_failed)?,
    };
    for row in &fk_rows {
        let table_name: String = pg_col(row, 0)?;
        let Some(definition) = tables.get_mut(&table_name) else {
            continue;
        };
        let name: String = pg_col(row, 1)?;
        let ref_schema: String = pg_col(row, 2)?;
        let ref_table: String = pg_col(row, 3)?;
        let column_name: String = pg_col(row, 4)?;
        let ref_column: String = pg_col(row, 5)?;
        let on_delete: String = pg_col(row, 6)?;
        let on_update: String = pg_col(row, 7)?;
        push_fk(
            definition,
            FkRow {
                name,
                ref_schema,
                ref_table,
                column: column_name,
                ref_column,
                on_delete: fk_action(&on_delete),
                on_update: fk_action(&on_update),
            },
        );
    }
    let constraint_rows = match table {
        Some(table) => client
            .query(CONSTRAINTS_ONE, &[&schema, &table])
            .await
            .map_err(query_failed)?,
        None => client
            .query(CONSTRAINTS_ALL, &[&schema])
            .await
            .map_err(query_failed)?,
    };
    for row in &constraint_rows {
        let table_name: String = pg_col(row, 0)?;
        let Some(definition) = tables.get_mut(&table_name) else {
            continue;
        };
        let name: String = pg_col(row, 1)?;
        let kind: String = pg_col(row, 2)?;
        let text: String = pg_col(row, 3)?;
        let columns: Vec<String> = pg_col(row, 4)?;
        let constraint = if kind == "c" {
            ConstraintDraft::loaded("check", name, Vec::new(), Some(check_expression(&text)))
        } else {
            ConstraintDraft::loaded("unique", name, columns, None)
        };
        definition.constraints.push(constraint);
    }
    Ok(tables.into_values().collect())
}

fn push_index(
    definition: &mut TableDefinition,
    name: String,
    unique: bool,
    primary: bool,
    column: String,
    index_sql: String,
) {
    if let Some(index) = definition
        .indexes
        .iter_mut()
        .find(|index| index.name == name)
    {
        index.columns.push(column.clone());
        if let Some(original) = index.original.as_mut() {
            original.columns.push(column);
        }
        return;
    }
    // `pg_get_indexdef`: partial, expression, and INCLUDE indexes replay as they are.
    definition.indexes.push(
        IndexDraft::loaded(name, vec![column], unique, primary).with_definition(Some(index_sql)),
    );
}

struct FkRow {
    name: String,
    ref_schema: String,
    ref_table: String,
    column: String,
    ref_column: String,
    on_delete: Option<String>,
    on_update: Option<String>,
}

/// `pg_constraint.confdeltype` / `confupdtype` as SQL.
fn fk_action(code: &str) -> Option<String> {
    match code {
        "r" => Some("RESTRICT".into()),
        "c" => Some("CASCADE".into()),
        "n" => Some("SET NULL".into()),
        "d" => Some("SET DEFAULT".into()),
        _ => None,
    }
}

fn push_fk(definition: &mut TableDefinition, row: FkRow) {
    if let Some(fk) = definition
        .foreign_keys
        .iter_mut()
        .find(|fk| fk.name == row.name)
    {
        fk.columns.push(row.column.clone());
        fk.ref_columns.push(row.ref_column.clone());
        if let Some(original) = fk.original.as_mut() {
            original.columns.push(row.column);
            original.ref_columns.push(row.ref_column);
        }
        return;
    }
    definition.foreign_keys.push(
        ForeignKeyDraft::loaded(
            row.name,
            vec![row.column],
            Some(row.ref_schema),
            row.ref_table,
            vec![row.ref_column],
        )
        .with_actions(row.on_delete, row.on_update),
    );
}

/// `CHECK ((a > 0))` from `pg_get_constraintdef` → `(a > 0)`.
fn check_expression(definition: &str) -> String {
    let text = definition.trim();
    let text = text.strip_prefix("CHECK").unwrap_or(text).trim();
    let text = text.strip_suffix("NOT VALID").unwrap_or(text).trim();
    let text = text.strip_suffix("NO INHERIT").unwrap_or(text).trim();
    let inner = text
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
        .filter(|inner| balanced(inner));
    inner.unwrap_or(text).trim().to_string()
}

fn balanced(text: &str) -> bool {
    let mut depth = 0i32;
    let mut quoted = false;
    for ch in text.chars() {
        match ch {
            '\'' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

fn empty_result() -> ApplyDdlResult {
    ApplyDdlResult {
        applied: Vec::new(),
        message: "Nothing to change.".into(),
    }
}

fn done(applied: Vec<String>) -> ApplyDdlResult {
    let count = applied.len();
    ApplyDdlResult {
        applied,
        message: format!("Applied {count} statement(s)."),
    }
}

fn partial_failure(applied: usize, err: &ConnectorError, rolled_back: bool) -> ConnectorError {
    let tail = if rolled_back {
        "The transaction was rolled back."
    } else if applied == 0 {
        "Nothing was applied."
    } else {
        "Earlier statements were not rolled back."
    };
    ConnectorError::Query(format!(
        "Applied {applied} statement(s), then failed. {tail} {err}"
    ))
}

const COLUMNS_ALL: &str = r#"
SELECT
  c.relname::text,
  a.attname::text,
  pg_catalog.format_type(a.atttypid, a.atttypmod),
  NOT a.attnotnull,
  COALESCE((
    SELECT k.ord::int4
    FROM pg_catalog.pg_index ix
    JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS k(attnum, ord)
      ON k.attnum = a.attnum
    WHERE ix.indrelid = c.oid AND ix.indisprimary
    LIMIT 1
  ), 0)::int4,
  CASE WHEN a.attgenerated = '' THEN pg_catalog.pg_get_expr(d.adbin, d.adrelid) END,
  a.attidentity::text
FROM pg_catalog.pg_class c
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
JOIN pg_catalog.pg_attribute a ON a.attrelid = c.oid
LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum
WHERE n.nspname = $1
  AND c.relkind IN ('r', 'p')
  AND a.attnum > 0
  AND NOT a.attisdropped
ORDER BY c.relname, a.attnum
"#;

const COLUMNS_ONE: &str = r#"
SELECT
  c.relname::text,
  a.attname::text,
  pg_catalog.format_type(a.atttypid, a.atttypmod),
  NOT a.attnotnull,
  COALESCE((
    SELECT k.ord::int4
    FROM pg_catalog.pg_index ix
    JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS k(attnum, ord)
      ON k.attnum = a.attnum
    WHERE ix.indrelid = c.oid AND ix.indisprimary
    LIMIT 1
  ), 0)::int4,
  CASE WHEN a.attgenerated = '' THEN pg_catalog.pg_get_expr(d.adbin, d.adrelid) END,
  a.attidentity::text
FROM pg_catalog.pg_class c
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
JOIN pg_catalog.pg_attribute a ON a.attrelid = c.oid
LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum
WHERE n.nspname = $1
  AND c.relname = $2
  AND c.relkind IN ('r', 'p')
  AND a.attnum > 0
  AND NOT a.attisdropped
ORDER BY a.attnum
"#;

const INDEXES_ALL: &str = r#"
SELECT
  t.relname::text,
  i.relname::text,
  ix.indisunique,
  ix.indisprimary,
  COALESCE(a.attname::text, '(expression)'),
  pg_catalog.pg_get_indexdef(ix.indexrelid)
FROM pg_catalog.pg_class t
JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace
JOIN pg_catalog.pg_index ix ON t.oid = ix.indrelid
JOIN pg_catalog.pg_class i ON i.oid = ix.indexrelid
JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS k(attnum, ord) ON true
LEFT JOIN pg_catalog.pg_attribute a ON a.attrelid = t.oid AND a.attnum = k.attnum AND k.attnum > 0
WHERE n.nspname = $1
  AND NOT EXISTS (
    SELECT 1 FROM pg_catalog.pg_constraint co
    WHERE co.conindid = ix.indexrelid AND co.contype IN ('u', 'x')
  )
  AND t.relkind IN ('r', 'p')
ORDER BY t.relname, i.relname, k.ord
"#;

const INDEXES_ONE: &str = r#"
SELECT
  t.relname::text,
  i.relname::text,
  ix.indisunique,
  ix.indisprimary,
  COALESCE(a.attname::text, '(expression)'),
  pg_catalog.pg_get_indexdef(ix.indexrelid)
FROM pg_catalog.pg_class t
JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace
JOIN pg_catalog.pg_index ix ON t.oid = ix.indrelid
JOIN pg_catalog.pg_class i ON i.oid = ix.indexrelid
JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS k(attnum, ord) ON true
LEFT JOIN pg_catalog.pg_attribute a ON a.attrelid = t.oid AND a.attnum = k.attnum AND k.attnum > 0
WHERE n.nspname = $1
  AND NOT EXISTS (
    SELECT 1 FROM pg_catalog.pg_constraint co
    WHERE co.conindid = ix.indexrelid AND co.contype IN ('u', 'x')
  )
  AND t.relname = $2
  AND t.relkind IN ('r', 'p')
ORDER BY i.relname, k.ord
"#;

const FKS_ALL: &str = r#"
SELECT
  rel.relname::text,
  con.conname::text,
  nsp_ref.nspname::text,
  rel_ref.relname::text,
  att.attname::text,
  att_ref.attname::text,
  con.confdeltype::text,
  con.confupdtype::text
FROM pg_catalog.pg_constraint con
JOIN pg_catalog.pg_class rel ON rel.oid = con.conrelid
JOIN pg_catalog.pg_namespace nsp ON nsp.oid = rel.relnamespace
JOIN LATERAL unnest(con.conkey, con.confkey) WITH ORDINALITY AS cols(attnum, ref_attnum, ord) ON true
JOIN pg_catalog.pg_attribute att
  ON att.attrelid = con.conrelid AND att.attnum = cols.attnum
JOIN pg_catalog.pg_class rel_ref ON rel_ref.oid = con.confrelid
JOIN pg_catalog.pg_namespace nsp_ref ON nsp_ref.oid = rel_ref.relnamespace
JOIN pg_catalog.pg_attribute att_ref
  ON att_ref.attrelid = con.confrelid AND att_ref.attnum = cols.ref_attnum
WHERE con.contype = 'f'
  AND nsp.nspname = $1
ORDER BY rel.relname, con.conname, cols.ord
"#;

const FKS_ONE: &str = r#"
SELECT
  rel.relname::text,
  con.conname::text,
  nsp_ref.nspname::text,
  rel_ref.relname::text,
  att.attname::text,
  att_ref.attname::text,
  con.confdeltype::text,
  con.confupdtype::text
FROM pg_catalog.pg_constraint con
JOIN pg_catalog.pg_class rel ON rel.oid = con.conrelid
JOIN pg_catalog.pg_namespace nsp ON nsp.oid = rel.relnamespace
JOIN LATERAL unnest(con.conkey, con.confkey) WITH ORDINALITY AS cols(attnum, ref_attnum, ord) ON true
JOIN pg_catalog.pg_attribute att
  ON att.attrelid = con.conrelid AND att.attnum = cols.attnum
JOIN pg_catalog.pg_class rel_ref ON rel_ref.oid = con.confrelid
JOIN pg_catalog.pg_namespace nsp_ref ON nsp_ref.oid = rel_ref.relnamespace
JOIN pg_catalog.pg_attribute att_ref
  ON att_ref.attrelid = con.confrelid AND att_ref.attnum = cols.ref_attnum
WHERE con.contype = 'f'
  AND nsp.nspname = $1
  AND rel.relname = $2
ORDER BY con.conname, cols.ord
"#;

const CONSTRAINTS_ALL: &str = r#"
SELECT
  rel.relname::text,
  con.conname::text,
  con.contype::text,
  pg_catalog.pg_get_constraintdef(con.oid),
  COALESCE((
    SELECT array_agg(att.attname::text ORDER BY k.ord)
    FROM unnest(con.conkey) WITH ORDINALITY AS k(attnum, ord)
    JOIN pg_catalog.pg_attribute att ON att.attrelid = con.conrelid AND att.attnum = k.attnum
  ), ARRAY[]::text[])
FROM pg_catalog.pg_constraint con
JOIN pg_catalog.pg_class rel ON rel.oid = con.conrelid
JOIN pg_catalog.pg_namespace nsp ON nsp.oid = rel.relnamespace
WHERE con.contype IN ('c', 'u')
  AND nsp.nspname = $1
ORDER BY rel.relname, con.conname
"#;

const CONSTRAINTS_ONE: &str = r#"
SELECT
  rel.relname::text,
  con.conname::text,
  con.contype::text,
  pg_catalog.pg_get_constraintdef(con.oid),
  COALESCE((
    SELECT array_agg(att.attname::text ORDER BY k.ord)
    FROM unnest(con.conkey) WITH ORDINALITY AS k(attnum, ord)
    JOIN pg_catalog.pg_attribute att ON att.attrelid = con.conrelid AND att.attnum = k.attnum
  ), ARRAY[]::text[])
FROM pg_catalog.pg_constraint con
JOIN pg_catalog.pg_class rel ON rel.oid = con.conrelid
JOIN pg_catalog.pg_namespace nsp ON nsp.oid = rel.relnamespace
WHERE con.contype IN ('c', 'u')
  AND nsp.nspname = $1
  AND rel.relname = $2
ORDER BY con.conname
"#;
