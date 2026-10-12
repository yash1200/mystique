//! Classifier holes that must not come back as reads.
//! See docs/PHASED_IMPLEMENTATION_PLAN.md SEC-01, 02, 03, 06, 08, 14, 19 and BE-33.

use mystique_sql::{classify, destructive_object_name, is_safe_dml, SqlDialect, StatementRisk};

fn pg(sql: &str) -> StatementRisk {
    classify(sql, SqlDialect::Postgres)
}

fn mysql(sql: &str) -> StatementRisk {
    classify(sql, SqlDialect::Mysql)
}

fn sqlite(sql: &str) -> StatementRisk {
    classify(sql, SqlDialect::Sqlite)
}

#[test]
fn data_modifying_cte_is_write() {
    assert_eq!(
        pg("WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d"),
        StatementRisk::Write
    );
    assert_eq!(
        pg("WITH d AS (INSERT INTO t VALUES (1) RETURNING *) SELECT * FROM d"),
        StatementRisk::Write
    );
    assert_eq!(
        pg("WITH d AS (UPDATE t SET a = 1 RETURNING *) SELECT * FROM d"),
        StatementRisk::Write
    );
    assert_eq!(
        pg("SELECT * FROM (WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d) s"),
        StatementRisk::Write
    );
    assert_eq!(
        pg("SELECT 1 UNION ALL SELECT * FROM (WITH d AS (DELETE FROM t RETURNING id) SELECT * FROM d) s"),
        StatementRisk::Write
    );
    assert!(!is_safe_dml(
        "WITH d AS (DELETE FROM t RETURNING *) INSERT INTO t SELECT * FROM d",
        SqlDialect::Postgres
    ));
}

#[test]
fn parenthesised_explain_analyze_follows_the_inner_statement() {
    assert_eq!(pg("EXPLAIN (ANALYZE) DELETE FROM t"), StatementRisk::Write);
    assert_eq!(
        pg("EXPLAIN (ANALYZE true, FORMAT JSON) DELETE FROM t"),
        StatementRisk::Write
    );
    assert_eq!(pg("EXPLAIN (ANALYZE) SELECT 1"), StatementRisk::Read);
    assert_eq!(pg("EXPLAIN SELECT 1"), StatementRisk::Read);
}

#[test]
fn mysql_versioned_comment_is_write() {
    assert_eq!(
        mysql("SELECT 1 /*!50000 ; DROP TABLE t */"),
        StatementRisk::Write
    );
    assert_eq!(mysql("SELECT 1 /*M! DROP TABLE t */"), StatementRisk::Write);
    assert_eq!(mysql("SELECT 1 /*T! DROP TABLE t */"), StatementRisk::Write);
    assert_eq!(mysql("SELECT '/*!50000 drop */'"), StatementRisk::Read);
}

#[test]
fn side_effect_functions_are_not_reads() {
    assert_eq!(
        pg("SELECT pg_terminate_backend(1)"),
        StatementRisk::Destructive
    );
    assert_eq!(
        pg("SELECT set_config('statement_timeout', '0', false)"),
        StatementRisk::Write
    );
    assert_eq!(
        pg("SELECT dblink_exec('c', 'DROP TABLE t')"),
        StatementRisk::Write
    );
    assert_eq!(pg("SELECT nextval('s')"), StatementRisk::Write);
    assert_eq!(pg("SELECT lo_import('/tmp/x')"), StatementRisk::Write);
    assert_eq!(pg("SELECT lo_export(1, '/tmp/x')"), StatementRisk::Write);
    assert_eq!(mysql("SELECT SLEEP(10)"), StatementRisk::Write);
    assert_eq!(mysql("SELECT GET_LOCK('a', 1)"), StatementRisk::Write);
    assert_eq!(mysql("SELECT BENCHMARK(1, 1)"), StatementRisk::Write);
    assert_eq!(
        sqlite("SELECT load_extension('x')"),
        StatementRisk::Destructive
    );
    assert_eq!(pg("SELECT count(*) FROM t"), StatementRisk::Read);
}

#[test]
fn privileged_set_is_write_and_search_path_stays_session() {
    assert_eq!(pg("SET GLOBAL statement_timeout = 0"), StatementRisk::Write);
    assert_eq!(pg("SET statement_timeout = 0"), StatementRisk::Write);
    assert_eq!(
        pg("SET default_transaction_read_only = off"),
        StatementRisk::Write
    );
    assert_eq!(pg("SET ROLE admin"), StatementRisk::Write);
    assert_eq!(
        mysql("SET PERSIST max_connections = 10"),
        StatementRisk::Write
    );
    assert!(pg("SET search_path TO public").is_read_like());
}

#[test]
fn merge_delete_is_not_safe_dml() {
    let sql = "MERGE INTO t USING s ON true WHEN MATCHED THEN DELETE";
    assert!(!is_safe_dml(sql, SqlDialect::Generic));
    assert_eq!(classify(sql, SqlDialect::Generic), StatementRisk::Write);
    assert!(is_safe_dml(
        "MERGE INTO t USING s ON t.id = s.id WHEN MATCHED THEN UPDATE SET a = s.a",
        SqlDialect::Generic
    ));
}

#[test]
fn destructive_confirm_lists_every_target() {
    assert_eq!(
        destructive_object_name("DROP TABLE important, trivial", SqlDialect::Postgres).as_deref(),
        Some("important, trivial")
    );
    assert_eq!(
        destructive_object_name("DROP TABLE a; DROP TABLE b", SqlDialect::Postgres).as_deref(),
        Some("a, b")
    );
    assert_eq!(
        destructive_object_name(r#"DROP TABLE "Important""#, SqlDialect::Postgres).as_deref(),
        Some("Important")
    );
    assert_eq!(
        destructive_object_name("DROP PROCEDURE purge_old", SqlDialect::Postgres).as_deref(),
        Some("purge_old")
    );
    assert_eq!(
        destructive_object_name("DROP TRIGGER tr ON t", SqlDialect::Postgres).as_deref(),
        Some("tr")
    );
    assert_eq!(
        destructive_object_name("ALTER TABLE t DROP COLUMN secret", SqlDialect::Postgres)
            .as_deref(),
        Some("secret")
    );
    assert_eq!(
        pg("ALTER TABLE t DROP COLUMN secret"),
        StatementRisk::Destructive
    );
    assert_eq!(pg("CALL purge_old()"), StatementRisk::Destructive);
    assert_eq!(
        destructive_object_name("CALL purge_old()", SqlDialect::Postgres).as_deref(),
        Some("purge_old")
    );
    assert_eq!(
        pg("DO $$ BEGIN DROP TABLE t; END $$"),
        StatementRisk::Destructive
    );
}

#[test]
fn table_and_pragma_table_info_are_reads_and_for_update_is_write() {
    assert_eq!(pg("TABLE t"), StatementRisk::Read, "TABLE t");
    assert_eq!(
        sqlite("PRAGMA table_info(t)"),
        StatementRisk::Read,
        "pragma"
    );
    assert_eq!(
        pg("SELECT * FROM (SELECT * FROM t FOR UPDATE) s"),
        StatementRisk::Write
    );
}
