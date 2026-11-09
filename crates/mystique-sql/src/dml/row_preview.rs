//! Affected-row preview for the production write dialog (F-05).
//!
//! A single-table `UPDATE` or `DELETE` becomes a `SELECT` with the same
//! table and `WHERE`, capped at `limit`, plus a `COUNT(*)`. An `UPDATE`
//! adds one `"<column> (new)"` column per assignment, the value the row
//! would get. `INSERT … VALUES` needs no query: its rows are read off the
//! statement (only the column names may need `SELECT * … WHERE 1 = 0`).
//!
//! The statements built here are reads. Core still runs them in Read mode,
//! inside the engine's read-only transaction, so a function call hidden in
//! the `WHERE` cannot write either.

use std::ops::ControlFlow;

use serde_json::Value as JsonValue;
use sqlparser::ast::{
    visit_expressions, AssignmentTarget, Expr, FromTable, ObjectNamePart, SetExpr, Statement,
    TableFactor, TableWithJoins, Value,
};
use sqlparser::parser::Parser;

use crate::{quote_ident, SqlDialect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowPreviewAction {
    Update,
    Delete,
    Insert,
}

impl RowPreviewAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Insert => "insert",
        }
    }
}

/// What to run (or show) to preview one write.
#[derive(Debug, Clone, PartialEq)]
pub struct RowPreviewPlan {
    pub action: RowPreviewAction,
    /// The table as written.
    pub table: String,
    /// UPDATE / DELETE: the rows the write would touch, at most `limit`.
    pub rows_sql: Option<String>,
    /// UPDATE / DELETE: how many rows the write would touch.
    pub count_sql: Option<String>,
    /// UPDATE: the `"<column> (new)"` labels, in assignment order.
    pub new_columns: Vec<String>,
    /// INSERT: the column list, empty when the statement has none.
    pub insert_columns: Vec<String>,
    /// INSERT without a column list: the table to describe for the names.
    pub insert_table: Option<TablePath>,
    /// INSERT: the rows to insert, literals as values and anything else as
    /// its SQL text.
    pub insert_rows: Vec<Vec<JsonValue>>,
}

/// A table name split the way the catalog takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TablePath {
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: String,
}

/// Build the preview for one statement, or say why there is none.
pub fn row_preview_plan(
    sql: &str,
    dialect: SqlDialect,
    limit: u32,
) -> Result<RowPreviewPlan, String> {
    if dialect == SqlDialect::Generic {
        return Err(UNSUPPORTED_ENGINE.into());
    }
    let parser = dialect.parser_dialect();
    let mut statements = Parser::parse_sql(&*parser, sql).map_err(|_| UNPARSED.to_string())?;
    if statements.len() != 1 {
        return Err(ONE_STATEMENT.into());
    }
    let statement = statements.remove(0);
    if has_placeholder(&statement) {
        return Err(PARAMETERS.into());
    }
    let limit = limit.clamp(1, 1_000);
    match statement {
        Statement::Update {
            table,
            assignments,
            from,
            selection,
            ..
        } => {
            if from.is_some() {
                return Err(SINGLE_TABLE.into());
            }
            let (relation, qualifier, display) = single_relation(&table)?;
            let mut new_columns = Vec::with_capacity(assignments.len());
            let mut select = vec![format!("{qualifier}.*")];
            for assignment in &assignments {
                let AssignmentTarget::ColumnName(name) = &assignment.target else {
                    return Err(SHAPE.into());
                };
                let Some(ObjectNamePart::Identifier(column)) = name.0.last() else {
                    return Err(SHAPE.into());
                };
                let label = format!("{} (new)", column.value);
                select.push(format!(
                    "{} AS {}",
                    assignment.value,
                    quote_ident(dialect, &label)
                ));
                new_columns.push(label);
            }
            let filter = where_clause(selection.as_ref());
            Ok(RowPreviewPlan {
                action: RowPreviewAction::Update,
                table: display,
                rows_sql: Some(format!(
                    "SELECT {} FROM {relation}{filter} LIMIT {limit}",
                    select.join(", ")
                )),
                count_sql: Some(format!("SELECT COUNT(*) FROM {relation}{filter}")),
                new_columns,
                insert_columns: Vec::new(),
                insert_table: None,
                insert_rows: Vec::new(),
            })
        }
        Statement::Delete(delete) => {
            if !delete.tables.is_empty() || delete.using.is_some() {
                return Err(SINGLE_TABLE.into());
            }
            if !delete.order_by.is_empty() || delete.limit.is_some() {
                return Err(LIMITED.into());
            }
            let from = match &delete.from {
                FromTable::WithFromKeyword(from) | FromTable::WithoutKeyword(from) => from,
            };
            let [table] = from.as_slice() else {
                return Err(SINGLE_TABLE.into());
            };
            let (relation, qualifier, display) = single_relation(table)?;
            let filter = where_clause(delete.selection.as_ref());
            Ok(RowPreviewPlan {
                action: RowPreviewAction::Delete,
                table: display,
                rows_sql: Some(format!(
                    "SELECT {qualifier}.* FROM {relation}{filter} LIMIT {limit}"
                )),
                count_sql: Some(format!("SELECT COUNT(*) FROM {relation}{filter}")),
                new_columns: Vec::new(),
                insert_columns: Vec::new(),
                insert_table: None,
                insert_rows: Vec::new(),
            })
        }
        Statement::Insert(insert) => {
            let sqlparser::ast::TableObject::TableName(name) = &insert.table else {
                return Err(SHAPE.into());
            };
            let Some(source) = insert.source.as_ref() else {
                return Err(INSERT_VALUES.into());
            };
            if source.with.is_some() || source.order_by.is_some() || source.limit_clause.is_some() {
                return Err(INSERT_VALUES.into());
            }
            let SetExpr::Values(values) = source.body.as_ref() else {
                return Err(INSERT_VALUES.into());
            };
            let insert_columns: Vec<String> =
                insert.columns.iter().map(|c| c.value.clone()).collect();
            let insert_table = if insert_columns.is_empty() {
                Some(table_path(name, dialect).ok_or_else(|| SHAPE.to_string())?)
            } else {
                None
            };
            let take = usize::try_from(limit).unwrap_or(usize::MAX);
            Ok(RowPreviewPlan {
                action: RowPreviewAction::Insert,
                table: name.to_string(),
                rows_sql: None,
                count_sql: None,
                new_columns: Vec::new(),
                insert_columns,
                insert_table,
                insert_rows: values
                    .rows
                    .iter()
                    .take(take)
                    .map(|row| row.iter().map(cell).collect())
                    .collect(),
            })
        }
        _ => Err(SHAPE.into()),
    }
}

/// How many rows an `INSERT … VALUES` adds, read off the statement.
pub fn insert_row_count(sql: &str, dialect: SqlDialect) -> Option<u64> {
    let parser = dialect.parser_dialect();
    let statements = Parser::parse_sql(&*parser, sql).ok()?;
    let [Statement::Insert(insert)] = statements.as_slice() else {
        return None;
    };
    let SetExpr::Values(values) = insert.source.as_ref()?.body.as_ref() else {
        return None;
    };
    u64::try_from(values.rows.len()).ok()
}

const UNSUPPORTED_ENGINE: &str = "Row preview is for Postgres, MySQL and SQLite.";
const UNPARSED: &str = "Row preview needs a statement it can read; this one did not parse.";
const ONE_STATEMENT: &str = "Row preview covers one statement at a time.";
const PARAMETERS: &str = "Row preview cannot run a statement with parameters.";
const SINGLE_TABLE: &str = "Row preview covers a write to one table (no joins, FROM or USING).";
const LIMITED: &str = "Row preview does not cover DELETE with ORDER BY or LIMIT.";
const INSERT_VALUES: &str = "Row preview covers INSERT … VALUES, not INSERT … SELECT.";
const SHAPE: &str = "Row preview covers single-table UPDATE, DELETE and INSERT … VALUES.";

/// `FROM` text, the `.*` qualifier, and the table name as written.
fn single_relation(table: &TableWithJoins) -> Result<(String, String, String), String> {
    if !table.joins.is_empty() {
        return Err(SINGLE_TABLE.into());
    }
    let TableFactor::Table {
        name, alias, args, ..
    } = &table.relation
    else {
        return Err(SINGLE_TABLE.into());
    };
    if args.is_some() {
        return Err(SINGLE_TABLE.into());
    }
    let qualifier = match alias {
        Some(alias) => alias.name.to_string(),
        None => name.to_string(),
    };
    Ok((table.relation.to_string(), qualifier, name.to_string()))
}

/// `d.s.t` (Postgres), `d.t` (MySQL) or `s.t` (Postgres, SQLite).
fn table_path(name: &sqlparser::ast::ObjectName, dialect: SqlDialect) -> Option<TablePath> {
    let parts: Vec<String> = name
        .0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(ident)
                if dialect == SqlDialect::Postgres && ident.quote_style.is_none() =>
            {
                Some(ident.value.to_lowercase())
            }
            ObjectNamePart::Identifier(ident) => Some(ident.value.clone()),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let path = |database: Option<&String>, schema: Option<&String>, table: &String| TablePath {
        database: database.cloned(),
        schema: schema.cloned(),
        table: table.clone(),
    };
    match (dialect, parts.as_slice()) {
        (_, [table]) => Some(path(None, None, table)),
        (SqlDialect::Mysql, [database, table]) => Some(path(Some(database), None, table)),
        (_, [schema, table]) => Some(path(None, Some(schema), table)),
        (SqlDialect::Postgres, [database, schema, table]) => {
            Some(path(Some(database), Some(schema), table))
        }
        _ => None,
    }
}

fn where_clause(selection: Option<&Expr>) -> String {
    selection
        .map(|expr| format!(" WHERE {expr}"))
        .unwrap_or_default()
}

fn has_placeholder(statement: &Statement) -> bool {
    visit_expressions(statement, |expr| match expr {
        Expr::Value(value) if matches!(value.value, Value::Placeholder(_)) => {
            ControlFlow::Break(())
        }
        _ => ControlFlow::Continue(()),
    })
    .is_break()
}

/// A literal as its value; anything else as SQL text.
fn cell(expr: &Expr) -> JsonValue {
    match expr {
        Expr::Value(value) => match &value.value {
            Value::Null => JsonValue::Null,
            Value::Boolean(b) => JsonValue::Bool(*b),
            Value::Number(n, _) => JsonValue::String(n.to_string()),
            other => match other.clone().into_string() {
                Some(text) => JsonValue::String(text),
                None => JsonValue::String(other.to_string()),
            },
        },
        other => JsonValue::String(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pg(sql: &str) -> RowPreviewPlan {
        row_preview_plan(sql, SqlDialect::Postgres, 50).expect("plan")
    }

    #[test]
    fn update_selects_the_rows_and_the_new_values() {
        let plan = pg("UPDATE t SET a = 1, b = b + 1 WHERE id = 2");
        assert_eq!(plan.action, RowPreviewAction::Update);
        assert_eq!(plan.table, "t");
        assert_eq!(
            plan.rows_sql.as_deref(),
            Some(r#"SELECT t.*, 1 AS "a (new)", b + 1 AS "b (new)" FROM t WHERE id = 2 LIMIT 50"#)
        );
        assert_eq!(
            plan.count_sql.as_deref(),
            Some("SELECT COUNT(*) FROM t WHERE id = 2")
        );
        assert_eq!(plan.new_columns, ["a (new)", "b (new)"]);
    }

    #[test]
    fn mysql_update_keeps_the_alias_and_backticks() {
        let plan = row_preview_plan(
            "UPDATE `prod`.`users` AS u SET u.`name` = 'x' WHERE u.id = 1",
            SqlDialect::Mysql,
            50,
        )
        .expect("plan");
        assert_eq!(
            plan.rows_sql.as_deref(),
            Some(
                "SELECT u.*, 'x' AS `name (new)` FROM `prod`.`users` AS u WHERE u.id = 1 LIMIT 50"
            )
        );
        assert_eq!(plan.table, "`prod`.`users`");
    }

    #[test]
    fn delete_selects_what_it_removes_and_no_where_counts_everything() {
        let plan = pg("DELETE FROM sales.orders WHERE total < 0");
        assert_eq!(
            plan.rows_sql.as_deref(),
            Some("SELECT sales.orders.* FROM sales.orders WHERE total < 0 LIMIT 50")
        );
        let plan = row_preview_plan("DELETE FROM t", SqlDialect::Sqlite, 10).expect("plan");
        assert_eq!(plan.count_sql.as_deref(), Some("SELECT COUNT(*) FROM t"));
        assert_eq!(plan.rows_sql.as_deref(), Some("SELECT t.* FROM t LIMIT 10"));
    }

    #[test]
    fn insert_values_are_read_off_the_statement() {
        let plan = pg("INSERT INTO t (id, name, ok, at) VALUES (1, 'a', true, now()), (2, NULL, false, now())");
        assert_eq!(plan.action, RowPreviewAction::Insert);
        assert_eq!(plan.rows_sql, None);
        assert_eq!(plan.insert_columns, ["id", "name", "ok", "at"]);
        assert_eq!(
            plan.insert_rows,
            vec![
                vec![json!("1"), json!("a"), json!(true), json!("now()")],
                vec![json!("2"), json!(null), json!(false), json!("now()")],
            ]
        );
        let plan = pg("INSERT INTO Sales.T VALUES (1)");
        assert_eq!(plan.rows_sql, None);
        assert_eq!(
            plan.insert_table,
            Some(TablePath {
                database: None,
                schema: Some("sales".into()),
                table: "t".into(),
            })
        );
        let plan =
            row_preview_plan("INSERT INTO prod.t VALUES (1)", SqlDialect::Mysql, 50).expect("plan");
        assert_eq!(
            plan.insert_table,
            Some(TablePath {
                database: Some("prod".into()),
                schema: None,
                table: "t".into(),
            })
        );
        assert_eq!(
            insert_row_count("INSERT INTO t VALUES (1), (2)", SqlDialect::Postgres),
            Some(2)
        );
    }

    #[test]
    fn unsupported_shapes_say_why() {
        let why = |sql: &str, dialect| row_preview_plan(sql, dialect, 50).expect_err(sql);
        assert!(why(
            "UPDATE t SET a = o.a FROM o WHERE o.id = t.id",
            SqlDialect::Postgres
        )
        .contains("one table"));
        assert!(why(
            "DELETE FROM t USING u WHERE t.id = u.id",
            SqlDialect::Postgres
        )
        .contains("one table"));
        assert!(
            why("DELETE a FROM a JOIN b ON a.id = b.id", SqlDialect::Mysql).contains("one table")
        );
        assert!(why("INSERT INTO t SELECT * FROM u", SqlDialect::Postgres).contains("VALUES"));
        assert!(
            why("UPDATE t SET a = $1 WHERE id = $2", SqlDialect::Postgres).contains("parameters")
        );
        assert!(why("DELETE FROM t; DELETE FROM u", SqlDialect::Postgres).contains("one statement"));
        assert!(why("DROP TABLE t", SqlDialect::Postgres).contains("single-table"));
        assert!(why("UPDATE t SET a = 1", SqlDialect::Generic).contains("Postgres"));
        assert!(why("DELETE FROM t WHERE id IN (", SqlDialect::Postgres).contains("parse"));
    }

    #[test]
    fn the_preview_is_a_read() {
        for sql in [
            "UPDATE t SET a = 1 WHERE id = 2",
            "DELETE FROM t WHERE id = 2",
        ] {
            let plan = pg(sql);
            for read in [plan.rows_sql.unwrap(), plan.count_sql.unwrap()] {
                assert_eq!(
                    crate::classify(&read, SqlDialect::Postgres),
                    crate::StatementRisk::Read,
                    "{read}"
                );
            }
        }
    }
}
