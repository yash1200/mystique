use mystique_sql::StatementRisk;

use crate::error::{ConnectorError, ConnectorResult};
use crate::query::QueryMode;
use crate::WriteRisk;

pub use mystique_sql::SqlDialect;

pub fn split_sql_statements(sql: &str) -> Vec<String> {
    split_sql_statements_for(sql, SqlDialect::Generic)
}

pub fn split_sql_statements_for(sql: &str, dialect: SqlDialect) -> Vec<String> {
    mystique_sql::split_statements(sql, dialect)
}

/// INSERT / UPDATE / REPLACE / MERGE only (P3.2 `safe_write`). All statements in a batch must match.
pub fn sql_explain_executes(text: &str, dialect: SqlDialect) -> bool {
    mystique_sql::explain_executes(text, dialect)
}

/// Inner statement the Explain button should plan.
pub fn sql_explain_subject(text: &str, dialect: SqlDialect) -> Result<String, String> {
    mystique_sql::explain_subject(text, dialect)
}

/// A proven read is `StatementRisk::Read`. `SET` and writes are not.
pub fn sql_is_proven_read(text: &str, dialect: SqlDialect) -> bool {
    mystique_sql::classify(text, dialect) == StatementRisk::Read
}

/// Guard risk for analyze. Session prompts on production; a plain read does not.
pub fn sql_analyze_guard_risk(text: &str, dialect: SqlDialect) -> WriteRisk {
    match mystique_sql::classify(text, dialect) {
        StatementRisk::Read => WriteRisk::Read,
        StatementRisk::Destructive => WriteRisk::Destructive,
        StatementRisk::Write | StatementRisk::Session => WriteRisk::Write,
    }
}

pub fn sql_is_safe_dml(text: &str) -> bool {
    sql_is_safe_dml_for(text, SqlDialect::Generic)
}

pub fn sql_is_safe_dml_for(text: &str, dialect: SqlDialect) -> bool {
    mystique_sql::is_safe_dml(text, dialect)
}

/// Worst risk across a SQL batch. Unparsed statements count as Write.
pub fn classify_sql_statement(text: &str) -> WriteRisk {
    classify_sql_statement_for(text, SqlDialect::Generic)
}

pub fn classify_sql_statement_for(text: &str, dialect: SqlDialect) -> WriteRisk {
    to_write_risk(mystique_sql::classify(text, dialect))
}

/// Last identifier after DROP/TRUNCATE for type-to-confirm.
pub fn sql_destructive_object_name(text: &str) -> Option<String> {
    sql_destructive_object_name_for(text, SqlDialect::Generic)
}

pub fn sql_destructive_object_name_for(text: &str, dialect: SqlDialect) -> Option<String> {
    mystique_sql::destructive_object_name(text, dialect)
}

fn to_write_risk(risk: StatementRisk) -> WriteRisk {
    match risk {
        StatementRisk::Read | StatementRisk::Session => WriteRisk::Read,
        StatementRisk::Write => WriteRisk::Write,
        StatementRisk::Destructive => WriteRisk::Destructive,
    }
}

fn deny_write(risk: StatementRisk) -> ConnectorError {
    let kind = match risk {
        StatementRisk::Destructive => "DROP/TRUNCATE",
        _ => "write",
    };
    ConnectorError::Policy(format!(
        "Write statement ({kind}) blocked. This connection is read-only."
    ))
}

/// Shared SQL policy. Multi-statement batches: every statement is classified.
pub fn validate_sql_statement(text: &str, mode: QueryMode) -> ConnectorResult<()> {
    validate_sql_statement_for(text, mode, SqlDialect::Generic)
}

pub fn validate_sql_statement_for(
    text: &str,
    mode: QueryMode,
    dialect: SqlDialect,
) -> ConnectorResult<()> {
    let stmts = mystique_sql::split_statements(text, dialect);
    if stmts.is_empty() {
        return Err(ConnectorError::Policy("Query is empty.".into()));
    }
    if mode != QueryMode::Read {
        return Ok(());
    }
    // Same rule as the batch gate: reads, plus a harmless session `SET`.
    for stmt in &stmts {
        let risk = mystique_sql::classify(stmt, dialect);
        let allowed = match risk {
            StatementRisk::Read => true,
            StatementRisk::Session => mystique_sql::is_read_safe_set(stmt, dialect),
            StatementRisk::Write | StatementRisk::Destructive => false,
        };
        if !allowed {
            return Err(deny_write(risk));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_select_is_read() {
        assert_eq!(classify_sql_statement("SELECT 1"), WriteRisk::Read);
    }

    #[test]
    fn classify_delete_is_write() {
        assert_eq!(
            classify_sql_statement("DELETE FROM users"),
            WriteRisk::Write
        );
    }

    #[test]
    fn classify_drop_is_destructive() {
        assert_eq!(
            classify_sql_statement("DROP TABLE public.users"),
            WriteRisk::Destructive
        );
        assert_eq!(
            sql_destructive_object_name("DROP TABLE public.users").as_deref(),
            Some("users")
        );
    }

    #[test]
    fn safe_dml_insert_yes_delete_no() {
        assert!(sql_is_safe_dml("INSERT INTO t VALUES (1)"));
        assert!(sql_is_safe_dml("UPDATE t SET a = 1"));
        assert!(!sql_is_safe_dml("DELETE FROM t"));
        assert!(!sql_is_safe_dml("CREATE TABLE t (a int)"));
        assert!(!sql_is_safe_dml("INSERT INTO t VALUES (1); DELETE FROM t"));
    }

    #[test]
    fn classify_batch_takes_worst() {
        assert_eq!(
            classify_sql_statement("SELECT 1; DELETE FROM t;"),
            WriteRisk::Write
        );
        assert_eq!(
            classify_sql_statement("INSERT INTO t VALUES (1); DROP TABLE t;"),
            WriteRisk::Destructive
        );
    }

    #[test]
    fn with_insert_denied_in_read_for_postgres_and_mysql() {
        let sql = "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x";
        assert!(validate_sql_statement_for(sql, QueryMode::Read, SqlDialect::Postgres).is_err());
        assert!(validate_sql_statement_for(sql, QueryMode::Read, SqlDialect::Mysql).is_err());
        assert!(validate_sql_statement_for(sql, QueryMode::Write, SqlDialect::Postgres).is_ok());
    }

    #[test]
    fn with_select_allowed_in_read() {
        let sql = "WITH cte AS (SELECT 1) SELECT * FROM cte";
        assert!(validate_sql_statement_for(sql, QueryMode::Read, SqlDialect::Postgres).is_ok());
    }

    #[test]
    fn select_into_and_explain_analyze_delete_denied_in_read() {
        assert!(validate_sql_statement_for(
            "SELECT * INTO new_table FROM old",
            QueryMode::Read,
            SqlDialect::Postgres
        )
        .is_err());
        assert!(validate_sql_statement_for(
            "EXPLAIN ANALYZE DELETE FROM t",
            QueryMode::Read,
            SqlDialect::Postgres
        )
        .is_err());
    }

    #[test]
    fn read_mode_allows_safe_set_and_not_transaction_or_role() {
        let runs =
            |sql| validate_sql_statement_for(sql, QueryMode::Read, SqlDialect::Postgres).is_ok();
        assert!(runs("SET search_path TO public"));
        assert!(runs("SET TIME ZONE 'UTC'"));
        assert!(!runs("SET TRANSACTION READ WRITE"));
        assert!(!runs("SET ROLE admin"));
        assert!(!runs("SET statement_timeout = 0"));
        assert!(!runs("DECLARE c CURSOR FOR SELECT 1"));
        assert!(validate_sql_statement_for(
            "SET ROLE admin",
            QueryMode::Write,
            SqlDialect::Postgres
        )
        .is_ok());
    }

    #[test]
    fn unparsed_is_a_write_and_never_safe_dml() {
        for sql in [
            "VACUUM",
            "REINDEX TABLE t",
            "INSERT INTO t VALUES (1); VACUUM",
        ] {
            assert_eq!(
                classify_sql_statement_for(sql, SqlDialect::Postgres),
                WriteRisk::Write,
                "{sql}"
            );
            assert!(!sql_is_safe_dml_for(sql, SqlDialect::Postgres), "{sql}");
        }
    }

    #[test]
    fn batch_write_fails_read_mode() {
        assert!(validate_sql_statement_for(
            "SELECT 1; DELETE FROM t;",
            QueryMode::Read,
            SqlDialect::Postgres
        )
        .is_err());
    }
}
