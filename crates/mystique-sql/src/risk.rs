//! AST write-risk for a SQL statement. Parse failure is Write (fail closed).

use sqlparser::ast::{ObjectName, Query, SetExpr, Statement};
use sqlparser::parser::Parser;

use super::{split_statements, SqlDialect};

/// Statement-level risk. Session (SET / BEGIN) is not a data write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StatementRisk {
    Read = 0,
    Session = 1,
    Write = 2,
    Destructive = 3,
}

impl StatementRisk {
    pub fn is_read_like(self) -> bool {
        matches!(self, Self::Read | Self::Session)
    }
}

/// Worst risk across the batch. Empty SQL is Read.
pub fn classify(sql: &str, dialect: SqlDialect) -> StatementRisk {
    let stmts = split_statements(sql, dialect);
    if stmts.is_empty() {
        return StatementRisk::Read;
    }
    let mut worst = StatementRisk::Read;
    for stmt in &stmts {
        let risk = classify_one(stmt, dialect);
        if risk > worst {
            worst = risk;
        }
    }
    worst
}

/// INSERT / UPDATE / MERGE / REPLACE only. All statements in the batch must match.
pub fn is_safe_dml(sql: &str, dialect: SqlDialect) -> bool {
    let stmts = split_statements(sql, dialect);
    if stmts.is_empty() {
        return false;
    }
    stmts.iter().all(|s| one_is_safe_dml(s, dialect))
}

/// Last identifier on a DROP / TRUNCATE in the batch (type-to-confirm).
pub fn destructive_object_name(sql: &str, dialect: SqlDialect) -> Option<String> {
    let stmts = split_statements(sql, dialect);
    for stmt in stmts.iter().rev() {
        if let Some(name) = one_destructive_name(stmt, dialect) {
            return Some(name);
        }
    }
    None
}

fn parse(sql: &str, dialect: SqlDialect) -> Option<Vec<Statement>> {
    let boxed = dialect.parser_dialect();
    Parser::parse_sql(&*boxed, sql).ok().or_else(|| {
        if matches!(dialect, SqlDialect::Generic) {
            None
        } else {
            Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, sql).ok()
        }
    })
}

fn classify_one(sql: &str, dialect: SqlDialect) -> StatementRisk {
    let Some(parsed) = parse(sql, dialect) else {
        return StatementRisk::Write;
    };
    let mut worst = StatementRisk::Read;
    for stmt in &parsed {
        let risk = classify_ast(stmt);
        if risk > worst {
            worst = risk;
        }
    }
    worst
}

fn one_is_safe_dml(sql: &str, dialect: SqlDialect) -> bool {
    let Some(parsed) = parse(sql, dialect) else {
        return false;
    };
    !parsed.is_empty() && parsed.iter().all(ast_is_safe_dml)
}

fn one_destructive_name(sql: &str, dialect: SqlDialect) -> Option<String> {
    let parsed = parse(sql, dialect)?;
    for stmt in parsed.iter().rev() {
        if let Some(name) = ast_destructive_name(stmt) {
            return Some(name);
        }
    }
    None
}

fn classify_ast(stmt: &Statement) -> StatementRisk {
    match stmt {
        Statement::Query(q) => classify_query(q),
        Statement::Explain {
            analyze, statement, ..
        } => {
            if *analyze {
                classify_ast(statement)
            } else {
                StatementRisk::Read
            }
        }
        Statement::ExplainTable { .. }
        | Statement::ShowVariable { .. }
        | Statement::ShowStatus { .. }
        | Statement::ShowVariables { .. }
        | Statement::ShowCreate { .. }
        | Statement::ShowColumns { .. }
        | Statement::ShowTables { .. }
        | Statement::ShowFunctions { .. }
        | Statement::ShowDatabases { .. }
        | Statement::ShowSchemas { .. }
        | Statement::ShowViews { .. }
        | Statement::ShowCollation { .. }
        | Statement::ShowObjects(_) => StatementRisk::Read,

        Statement::Insert(_)
        | Statement::Update { .. }
        | Statement::Delete(_)
        | Statement::Merge { .. } => StatementRisk::Write,
        Statement::Copy { .. } => StatementRisk::Write,
        Statement::Directory { .. } => StatementRisk::Write,

        Statement::Truncate { .. }
        | Statement::Drop { .. }
        | Statement::DropFunction { .. }
        | Statement::DropProcedure { .. }
        | Statement::DropTrigger { .. }
        | Statement::DropExtension { .. }
        | Statement::DropDomain(_)
        | Statement::DropPolicy { .. }
        | Statement::DropSecret { .. }
        | Statement::DropConnector { .. } => StatementRisk::Destructive,

        Statement::Set(_)
        | Statement::Use(_)
        | Statement::StartTransaction { .. }
        | Statement::Commit { .. }
        | Statement::Rollback { .. }
        | Statement::Discard { .. }
        | Statement::Deallocate { .. }
        | Statement::Prepare { .. }
        | Statement::Declare { .. }
        | Statement::Fetch { .. }
        | Statement::Close { .. }
        | Statement::Open(_)
        | Statement::AlterSession { .. } => StatementRisk::Session,

        // EXECUTE may run a prepared write — fail closed.
        Statement::Execute { .. } => StatementRisk::Write,

        Statement::CreateView { .. }
        | Statement::CreateTable(_)
        | Statement::CreateVirtualTable { .. }
        | Statement::CreateIndex(_)
        | Statement::CreateRole { .. }
        | Statement::CreateSchema { .. }
        | Statement::CreateDatabase { .. }
        | Statement::CreateFunction(_)
        | Statement::CreateProcedure { .. }
        | Statement::CreateTrigger { .. }
        | Statement::CreateExtension { .. }
        | Statement::CreateMacro { .. }
        | Statement::CreateStage { .. }
        | Statement::CreateSecret { .. }
        | Statement::CreatePolicy { .. }
        | Statement::CreateServer(_)
        | Statement::CreateConnector(_)
        | Statement::AlterTable { .. }
        | Statement::AlterIndex { .. }
        | Statement::AlterView { .. }
        | Statement::AlterType(_)
        | Statement::AlterRole { .. }
        | Statement::AlterPolicy { .. }
        | Statement::AlterConnector { .. }
        | Statement::Grant { .. }
        | Statement::Revoke { .. }
        | Statement::Deny(_)
        | Statement::Comment { .. }
        | Statement::Analyze { .. }
        | Statement::AttachDatabase { .. }
        | Statement::AttachDuckDBDatabase { .. }
        | Statement::DetachDuckDBDatabase { .. } => StatementRisk::Write,

        _ => StatementRisk::Write,
    }
}

fn classify_query(q: &Query) -> StatementRisk {
    if !q.locks.is_empty() {
        return StatementRisk::Write;
    }
    classify_set_expr(&q.body)
}

fn classify_set_expr(expr: &SetExpr) -> StatementRisk {
    match expr {
        SetExpr::Select(select) => {
            if select.into.is_some() {
                StatementRisk::Write
            } else {
                StatementRisk::Read
            }
        }
        SetExpr::Query(inner) => classify_query(inner),
        SetExpr::SetOperation { left, right, .. } => {
            classify_set_expr(left).max(classify_set_expr(right))
        }
        SetExpr::Values(_) => StatementRisk::Read,
        SetExpr::Table(_) => StatementRisk::Read,
        SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Delete(_) => StatementRisk::Write,
    }
}

fn ast_is_safe_dml(stmt: &Statement) -> bool {
    match stmt {
        Statement::Insert(_) | Statement::Update { .. } | Statement::Merge { .. } => true,
        Statement::Query(q) => matches!(q.body.as_ref(), SetExpr::Insert(_) | SetExpr::Update(_)),
        _ => false,
    }
}

fn ast_destructive_name(stmt: &Statement) -> Option<String> {
    match stmt {
        Statement::Drop { names, .. } => names.last().and_then(last_object_ident),
        Statement::Truncate { table_names, .. } => {
            table_names.last().and_then(|t| last_object_ident(&t.name))
        }
        Statement::DropFunction { func_desc, .. } => {
            func_desc.last().and_then(|f| last_object_ident(&f.name))
        }
        Statement::Explain {
            analyze: true,
            statement,
            ..
        } => ast_destructive_name(statement),
        _ => None,
    }
}

fn last_object_ident(name: &ObjectName) -> Option<String> {
    let ident = name.0.last()?;
    let raw = ident.to_string();
    let trimmed = raw.trim_matches(|c| matches!(c, '"' | '`' | '[' | ']'));
    let last = trimmed.rsplit('.').next().unwrap_or(trimmed);
    if last.is_empty() {
        None
    } else {
        Some(last.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg(sql: &str) -> StatementRisk {
        classify(sql, SqlDialect::Postgres)
    }

    fn mysql(sql: &str) -> StatementRisk {
        classify(sql, SqlDialect::Mysql)
    }

    #[test]
    fn select_is_read() {
        assert_eq!(pg("SELECT 1"), StatementRisk::Read);
    }

    #[test]
    fn with_select_is_read() {
        assert_eq!(
            pg("WITH cte AS (SELECT 1) SELECT * FROM cte"),
            StatementRisk::Read
        );
    }

    #[test]
    fn with_insert_is_write() {
        assert_eq!(
            pg("WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x"),
            StatementRisk::Write
        );
    }

    #[test]
    fn select_into_is_write() {
        assert_eq!(pg("SELECT * INTO new_table FROM old"), StatementRisk::Write);
        assert_eq!(
            classify("SELECT * INTO new_table FROM old", SqlDialect::Generic),
            StatementRisk::Write
        );
    }

    #[test]
    fn explain_analyze_delete_is_write() {
        assert_eq!(pg("EXPLAIN ANALYZE DELETE FROM t"), StatementRisk::Write);
    }

    #[test]
    fn explain_select_is_read() {
        assert_eq!(pg("EXPLAIN SELECT 1"), StatementRisk::Read);
    }

    #[test]
    fn delete_is_write_drop_is_destructive() {
        assert_eq!(pg("DELETE FROM users"), StatementRisk::Write);
        assert_eq!(pg("DROP TABLE public.users"), StatementRisk::Destructive);
        assert_eq!(
            destructive_object_name("DROP TABLE public.users", SqlDialect::Postgres).as_deref(),
            Some("users")
        );
    }

    #[test]
    fn batch_takes_worst() {
        assert_eq!(pg("SELECT 1; DELETE FROM t;"), StatementRisk::Write);
        assert_eq!(
            pg("INSERT INTO t VALUES (1); DROP TABLE t;"),
            StatementRisk::Destructive
        );
    }

    #[test]
    fn mysql_with_insert_is_write() {
        assert_eq!(
            mysql("WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x"),
            StatementRisk::Write
        );
        assert!(mysql("SELECT 1 FROM t").is_read_like());
    }

    #[test]
    fn safe_dml_insert_not_delete() {
        assert!(is_safe_dml(
            "INSERT INTO t VALUES (1)",
            SqlDialect::Postgres
        ));
        assert!(is_safe_dml("UPDATE t SET a = 1", SqlDialect::Postgres));
        assert!(!is_safe_dml("DELETE FROM t", SqlDialect::Postgres));
        assert!(!is_safe_dml("CREATE TABLE t (a int)", SqlDialect::Postgres));
        assert!(!is_safe_dml(
            "INSERT INTO t VALUES (1); DELETE FROM t",
            SqlDialect::Postgres
        ));
    }

    #[test]
    fn merge_is_write_and_safe_dml() {
        let sql = "MERGE INTO t USING s ON t.id = s.id WHEN MATCHED THEN UPDATE SET a = s.a";
        assert_eq!(classify(sql, SqlDialect::Generic), StatementRisk::Write);
        assert!(is_safe_dml(sql, SqlDialect::Generic));
    }

    #[test]
    fn set_is_session_read_like() {
        assert!(pg("SET search_path TO public").is_read_like());
    }
}
