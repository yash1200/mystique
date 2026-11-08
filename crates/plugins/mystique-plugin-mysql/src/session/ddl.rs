//! Read table definitions and run a DDL plan on one connection.

use std::collections::{BTreeMap, HashMap};

use mysql_async::prelude::Queryable;
use mystique_connector_api::{
    qualify_table, ApplyDdlResult, ColumnDraft, ConnectorError, ConnectorResult, DdlPlan,
    ForeignKeyDraft, IndexDraft, SqlIdentStyle, TableDefinition,
};

use super::query_failed;
use super::MysqlSession;

type FkRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

pub(super) async fn describe_table_definition(
    session: &MysqlSession,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> ConnectorResult<TableDefinition> {
    let db = schema_name(database, schema)?;
    let mut tables = load(session, db, Some(table)).await?;
    match tables.pop() {
        Some(definition) => Ok(definition),
        None => Err(ConnectorError::Query(format!(
            "Table `{db}.{table}` was not found."
        ))),
    }
}

pub(super) async fn list_table_definitions(
    session: &MysqlSession,
    database: Option<&str>,
    schema: Option<&str>,
) -> ConnectorResult<Vec<TableDefinition>> {
    let db = schema_name(database, schema)?;
    load(session, db, None).await
}

pub(super) async fn apply_ddl(
    session: &MysqlSession,
    plan: &DdlPlan,
    draft: &TableDefinition,
) -> ConnectorResult<ApplyDdlResult> {
    session.ensure_open()?;
    if plan.statements.is_empty() {
        return Ok(ApplyDdlResult {
            applied: Vec::new(),
            message: "Nothing to change.".into(),
        });
    }
    let mut conn = session.conn().await?;
    ensure_definitions_current(&mut conn, draft).await?;
    let mut applied = Vec::new();
    for sql in &plan.statements {
        if let Err(err) = conn.query_drop(sql.as_str()).await {
            let tail = if applied.is_empty() {
                "Nothing was applied."
            } else {
                "Earlier statements were not rolled back."
            };
            return Err(ConnectorError::Query(format!(
                "Applied {} statement(s), then failed. {tail} {}",
                applied.len(),
                query_failed(err)
            )));
        }
        applied.push(sql.clone());
    }
    let count = applied.len();
    Ok(ApplyDdlResult {
        applied,
        message: format!("Applied {count} statement(s)."),
    })
}

async fn load(
    session: &MysqlSession,
    database: &str,
    table: Option<&str>,
) -> ConnectorResult<Vec<TableDefinition>> {
    session.ensure_open()?;
    let mut conn = session.conn().await?;
    let mut tables: BTreeMap<String, TableDefinition> = BTreeMap::new();
    let column_rows: Vec<(String, String, String, String)> = match table {
        Some(table) => crate::rows::collect_exec(&mut conn, COLUMNS_ONE, (database, table)).await?,
        None => crate::rows::collect_exec(&mut conn, COLUMNS_ALL, (database,)).await?,
    };
    for (table_name, column_name, data_type, is_nullable) in column_rows {
        let definition = tables.entry(table_name.clone()).or_insert_with(|| {
            TableDefinition::loaded(Some(database.to_string()), None, table_name, "table")
        });
        definition.columns.push(ColumnDraft::loaded(
            column_name,
            data_type,
            !is_nullable.eq_ignore_ascii_case("NO"),
            0,
        ));
    }

    let index_rows: Vec<(String, String, Option<String>, i64, i64)> = match table {
        Some(table) => crate::rows::collect_exec(&mut conn, INDEXES_ONE, (database, table)).await?,
        None => crate::rows::collect_exec(&mut conn, INDEXES_ALL, (database,)).await?,
    };
    let mut pk: HashMap<(String, String), u32> = HashMap::new();
    for (table_name, index_name, column_name, non_unique, seq) in index_rows {
        let Some(column_name) = column_name.filter(|name| !name.is_empty()) else {
            continue;
        };
        let Some(definition) = tables.get_mut(&table_name) else {
            continue;
        };
        let primary = index_name.eq_ignore_ascii_case("PRIMARY");
        if primary {
            let position = u32::try_from(seq).unwrap_or(0);
            pk.insert((table_name, column_name.clone()), position);
        }
        push_index(
            definition,
            index_name,
            non_unique == 0,
            primary,
            column_name,
        );
    }
    for definition in tables.values_mut() {
        let table_name = definition.name.clone();
        for column in &mut definition.columns {
            let key = (table_name.clone(), column.name.clone());
            if let Some(position) = pk.get(&key).copied() {
                column.pk_position = position;
                if let Some(original) = column.original.as_mut() {
                    original.pk_position = position;
                }
            }
        }
    }

    let fk_rows: Vec<FkRow> = match table {
        Some(table) => crate::rows::collect_exec(&mut conn, FKS_ONE, (database, table)).await?,
        None => crate::rows::collect_exec(&mut conn, FKS_ALL, (database,)).await?,
    };
    for (table_name, constraint, column, ref_schema, ref_table, ref_column) in fk_rows {
        let (Some(column), Some(ref_table), Some(ref_column)) = (column, ref_table, ref_column)
        else {
            continue;
        };
        let Some(definition) = tables.get_mut(&table_name) else {
            continue;
        };
        push_fk(
            definition,
            constraint,
            ref_schema.filter(|schema| !schema.is_empty()),
            ref_table,
            column,
            ref_column,
        );
    }
    for definition in tables.values_mut() {
        let live = column_definitions(&mut conn, database, &definition.name).await?;
        for column in &mut definition.columns {
            if let Some(original) = column.original.as_mut() {
                original.definition = live.get(&original.name).cloned();
            }
        }
    }
    Ok(tables.into_values().collect())
}

/// MODIFY is built from the definitions read with the draft. Refuse when the
/// live table no longer matches them, so a stale or edited draft cannot
/// rewrite a column.
async fn ensure_definitions_current(
    conn: &mut mysql_async::Conn,
    draft: &TableDefinition,
) -> ConnectorResult<()> {
    if draft.create || draft.drop_table {
        return Ok(());
    }
    let expected: Vec<(&str, &str)> = draft
        .columns
        .iter()
        .filter_map(|column| column.original.as_ref())
        .filter_map(|original| {
            original
                .definition
                .as_deref()
                .map(|definition| (original.name.as_str(), definition))
        })
        .collect();
    if expected.is_empty() {
        return Ok(());
    }
    let database = schema_name(draft.database.as_deref(), draft.schema.as_deref())?;
    let live = column_definitions(conn, database, &draft.name).await?;
    for (name, definition) in expected {
        if live.get(name).map(String::as_str) != Some(definition.trim()) {
            return Err(ConnectorError::Query(format!(
                "Column {name} changed since the structure was loaded. Reload it and try again."
            )));
        }
    }
    Ok(())
}

/// Column definitions as `SHOW CREATE TABLE` prints them, keyed by name.
async fn column_definitions(
    conn: &mut mysql_async::Conn,
    database: &str,
    table: &str,
) -> ConnectorResult<HashMap<String, String>> {
    let name = qualify_table(SqlIdentStyle::Mysql, Some(database), None, table);
    let row = crate::rows::query_first_opt::<(String, String)>(
        conn,
        &format!("SHOW CREATE TABLE {name}"),
    )
    .await?;
    Ok(row
        .map(|(_, create)| parse_column_definitions(&create))
        .unwrap_or_default())
}

/// Column lines of a `CREATE TABLE`: two spaces, the quoted name, the rest.
fn parse_column_definitions(create: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in create.lines() {
        let Some(rest) = line.trim_start().strip_prefix('`') else {
            continue;
        };
        let mut name = String::new();
        let mut chars = rest.char_indices().peekable();
        let mut end = None;
        while let Some((at, ch)) = chars.next() {
            if ch == '`' {
                if chars.peek().map(|(_, next)| *next) == Some('`') {
                    name.push('`');
                    chars.next();
                    continue;
                }
                end = Some(at + 1);
                break;
            }
            name.push(ch);
        }
        let Some(definition) = end.and_then(|end| rest.get(end..)) else {
            continue;
        };
        let definition = definition.trim().trim_end_matches(',').trim();
        out.insert(name, definition.to_string());
    }
    out
}

fn push_index(
    definition: &mut TableDefinition,
    name: String,
    unique: bool,
    primary: bool,
    column: String,
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
    definition
        .indexes
        .push(IndexDraft::loaded(name, vec![column], unique, primary));
}

fn push_fk(
    definition: &mut TableDefinition,
    name: String,
    ref_schema: Option<String>,
    ref_table: String,
    column: String,
    ref_column: String,
) {
    if let Some(fk) = definition
        .foreign_keys
        .iter_mut()
        .find(|fk| fk.name == name)
    {
        fk.columns.push(column.clone());
        fk.ref_columns.push(ref_column.clone());
        if let Some(original) = fk.original.as_mut() {
            original.columns.push(column);
            original.ref_columns.push(ref_column);
        }
        return;
    }
    definition.foreign_keys.push(ForeignKeyDraft::loaded(
        name,
        vec![column],
        ref_schema,
        ref_table,
        vec![ref_column],
    ));
}

fn schema_name<'a>(database: Option<&'a str>, schema: Option<&'a str>) -> ConnectorResult<&'a str> {
    database
        .or(schema)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            ConnectorError::InvalidConfig("MySQL structure editing requires a database.".into())
        })
}

const COLUMNS_ALL: &str = "\
SELECT c.table_name, c.column_name, c.column_type, c.is_nullable \
FROM information_schema.columns c \
JOIN information_schema.tables t \
  ON t.table_schema = c.table_schema AND t.table_name = c.table_name \
WHERE c.table_schema = ? AND t.table_type = 'BASE TABLE' \
ORDER BY c.table_name, c.ordinal_position";

const COLUMNS_ONE: &str = "\
SELECT c.table_name, c.column_name, c.column_type, c.is_nullable \
FROM information_schema.columns c \
JOIN information_schema.tables t \
  ON t.table_schema = c.table_schema AND t.table_name = c.table_name \
WHERE c.table_schema = ? AND c.table_name = ? AND t.table_type = 'BASE TABLE' \
ORDER BY c.ordinal_position";

const INDEXES_ALL: &str = "\
SELECT table_name, index_name, column_name, non_unique, seq_in_index \
FROM information_schema.statistics \
WHERE table_schema = ? \
ORDER BY table_name, index_name, seq_in_index";

const INDEXES_ONE: &str = "\
SELECT table_name, index_name, column_name, non_unique, seq_in_index \
FROM information_schema.statistics \
WHERE table_schema = ? AND table_name = ? \
ORDER BY index_name, seq_in_index";

const FKS_ALL: &str = "\
SELECT table_name, constraint_name, column_name, referenced_table_schema, \
       referenced_table_name, referenced_column_name \
FROM information_schema.key_column_usage \
WHERE table_schema = ? AND referenced_table_name IS NOT NULL \
ORDER BY table_name, constraint_name, ordinal_position";

const FKS_ONE: &str = "\
SELECT table_name, constraint_name, column_name, referenced_table_schema, \
       referenced_table_name, referenced_column_name \
FROM information_schema.key_column_usage \
WHERE table_schema = ? AND table_name = ? AND referenced_table_name IS NOT NULL \
ORDER BY constraint_name, ordinal_position";

#[cfg(test)]
mod tests {
    use super::parse_column_definitions;

    #[test]
    fn create_table_column_lines_become_definitions() {
        let create = "CREATE TABLE `t` (\n  `id` int NOT NULL AUTO_INCREMENT,\n  `we``ird` varchar(20) CHARACTER SET latin1 DEFAULT 'a,b' COMMENT 'x',\n  PRIMARY KEY (`id`),\n  KEY `k` (`we``ird`)\n) ENGINE=InnoDB";
        let defs = parse_column_definitions(create);
        assert_eq!(defs.len(), 2, "{defs:?}");
        assert_eq!(
            defs.get("id").map(String::as_str),
            Some("int NOT NULL AUTO_INCREMENT")
        );
        assert_eq!(
            defs.get("we`ird").map(String::as_str),
            Some("varchar(20) CHARACTER SET latin1 DEFAULT 'a,b' COMMENT 'x'")
        );
    }
}
