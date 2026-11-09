//! Probe corpus for the statement classifier and the Read-mode gate.
//!
//! One row per probe. Add a row when a new bypass is found. See
//! docs/PHASED_PLAN_V3.md R-04, R-09 … R-14 and B11. The Mongo probes
//! (R-15, R-17) live with the Mongo shell parser in
//! `mystique-plugin-mongodb/src/shell/tests.rs`.

use mystique_connector_api::StatementKind::{
    Destructive as X, Read as R, Session as S, Write as W,
};
use mystique_connector_api::{
    gate_sql_batch, gate_sql_batch_in, sql_is_safe_dml_for, EngineQueryPolicy, IpcError,
    IpcErrorKind, QueryMode, SqlDialect, SqlQueryPolicy, StatementKind,
};

const PG: SqlDialect = SqlDialect::Postgres;
const MY: SqlDialect = SqlDialect::Mysql;
const LITE: SqlDialect = SqlDialect::Sqlite;

/// Read mode runs it.
const ALLOW: bool = true;
/// Read mode refuses it before anything runs.
const DENY: bool = false;
/// MCP / agent `safe_write` accepts it.
const SAFE: bool = true;
const UNSAFE: bool = false;

/// (statement, dialect, worst kind in Write mode, Read mode, `safe_write`)
#[rustfmt::skip]
const SQL: &[(&str, SqlDialect, StatementKind, bool, bool)] = &[
    // R-04: unparseable text is an opaque write in Write mode, refused in Read mode.
    ("VACUUM", PG, W, DENY, UNSAFE),
    ("VACUUM ANALYZE t", PG, W, DENY, UNSAFE),
    ("REINDEX TABLE t", PG, W, DENY, UNSAFE),
    ("VACUUM", LITE, W, DENY, UNSAFE),
    ("SELECT * FORM t", PG, W, DENY, UNSAFE),
    ("SELECT 1; VACUUM", PG, W, DENY, UNSAFE),
    ("INSERT INTO t VALUES (1); VACUUM", PG, W, DENY, UNSAFE),
    ("CREATE POLICY p ON t USING (true)", PG, W, DENY, UNSAFE),
    ("OPTIMIZE TABLE t", MY, W, DENY, UNSAFE),
    ("ATTACH DATABASE 'x.db' AS x", LITE, W, DENY, UNSAFE),

    // R-09: EXPLAIN runs its statement unless every option is provably non-executing.
    ("EXPLAIN (ANALYSE) DELETE FROM t", PG, W, DENY, UNSAFE),
    ("EXPLAIN (analyse true) DELETE FROM t", PG, W, DENY, UNSAFE),
    ("EXPLAIN (ANALYZE) DELETE FROM t", PG, W, DENY, UNSAFE),
    ("EXPLAIN (ANALYZE, BUFFERS) DELETE FROM t", PG, W, DENY, UNSAFE),
    ("EXPLAIN (ANALYZE 'off') DELETE FROM t", PG, W, DENY, UNSAFE),
    ("EXPLAIN (FOO) DELETE FROM t", PG, W, DENY, UNSAFE),
    ("EXPLAIN ANALYSE DELETE FROM t", PG, W, DENY, UNSAFE),
    ("EXPLAIN ANALYZE DELETE FROM t", PG, W, DENY, UNSAFE),
    ("EXPLAIN (ANALYSE) SELECT 1", PG, R, ALLOW, UNSAFE),
    ("EXPLAIN (ANALYZE false) DELETE FROM t", PG, R, ALLOW, UNSAFE),
    ("EXPLAIN (VERBOSE, COSTS off, FORMAT JSON) DELETE FROM t", PG, R, ALLOW, UNSAFE),
    ("EXPLAIN DELETE FROM t", PG, R, ALLOW, UNSAFE),
    ("EXPLAIN SELECT pg_sleep(1)", PG, R, ALLOW, UNSAFE),
    ("EXPLAIN ANALYZE SELECT 1", MY, R, ALLOW, UNSAFE),
    ("EXPLAIN QUERY PLAN SELECT 1", LITE, R, ALLOW, UNSAFE),

    // R-10: dblink and fdw calls, also as table functions.
    ("SELECT * FROM dblink('c', 'DELETE FROM t') AS x(a int)", PG, W, DENY, UNSAFE),
    ("SELECT 1 ORDER BY (SELECT dblink_exec('c', 'DELETE FROM t'))", PG, W, DENY, UNSAFE),
    ("SELECT * FROM t JOIN LATERAL dblink('c', 'DELETE FROM t') AS x(a int) ON true", PG, W, DENY, UNSAFE),
    ("SELECT * FROM (t JOIN dblink('c', 'x') AS d(a int) ON true)", PG, W, DENY, UNSAFE),
    ("SELECT dblink_send_query('c', 'DELETE FROM t')", PG, W, DENY, UNSAFE),
    ("SELECT public.dblink_exec('c', 'DELETE FROM t')", PG, W, DENY, UNSAFE),
    ("SELECT postgres_fdw_disconnect_all()", PG, W, DENY, UNSAFE),
    ("SELECT * FROM generate_series(1, 3) g", PG, R, ALLOW, UNSAFE),

    // R-11: any versioned comment fails closed, whatever quotes come before it.
    ("SELECT \"it's\" /*!50000 , 1 */", MY, W, DENY, UNSAFE),
    ("SELECT 'it\\'s' /*!50000 , 1 */", MY, W, DENY, UNSAFE),
    ("SELECT `a'b` /*!50000 , 1 */ FROM t", MY, W, DENY, UNSAFE),
    ("SELECT 1 # it's\n /*!50000 , 1 */", MY, W, DENY, UNSAFE),
    ("SELECT 1 -- it's\n /*!50000 , 1 */", MY, W, DENY, UNSAFE),
    ("SELECT 1 /*M! , 1 */", MY, W, DENY, UNSAFE),
    ("SELECT 1 /*T! , 1 */", MY, W, DENY, UNSAFE),
    ("SELECT '/*!50000 x */'", MY, W, DENY, UNSAFE),
    ("SELECT 1 /* it's a plain comment */", MY, R, ALLOW, UNSAFE),

    // R-12: every expression is walked; side-effect functions are not reads.
    ("SELECT ARRAY[pg_terminate_backend(1)]", PG, X, DENY, UNSAFE),
    ("SELECT 1 LIMIT (SELECT nextval('s'))", PG, W, DENY, UNSAFE),
    ("SELECT 1 OFFSET (SELECT nextval('s'))", PG, W, DENY, UNSAFE),
    ("SELECT a FROM t ORDER BY pg_sleep(1)", PG, W, DENY, UNSAFE),
    ("SELECT a FROM t GROUP BY a, pg_advisory_lock(1)", PG, W, DENY, UNSAFE),
    ("SELECT row_number() OVER (ORDER BY pg_sleep(1)) FROM t", PG, W, DENY, UNSAFE),
    ("SELECT sum(a) OVER w FROM t WINDOW w AS (PARTITION BY pg_sleep(1))", PG, W, DENY, UNSAFE),
    ("SELECT a FROM t WHERE a = ANY(ARRAY[pg_sleep(1)])", PG, W, DENY, UNSAFE),
    ("SELECT jsonb_build_object('a', pg_sleep(1))", PG, W, DENY, UNSAFE),
    ("SELECT * FROM t WHERE a IN (SELECT nextval('s'))", PG, W, DENY, UNSAFE),
    ("SELECT (pg_sleep(1))::text", PG, W, DENY, UNSAFE),
    ("SELECT pg_catalog.pg_sleep(1)", PG, W, DENY, UNSAFE),
    ("SELECT pg_advisory_lock(1)", PG, W, DENY, UNSAFE),
    ("SELECT pg_try_advisory_lock(1)", PG, W, DENY, UNSAFE),
    ("SELECT pg_advisory_xact_lock(1)", PG, W, DENY, UNSAFE),
    ("SELECT pg_advisory_unlock_all()", PG, W, DENY, UNSAFE),
    ("SELECT pg_sleep(1)", PG, W, DENY, UNSAFE),
    ("SELECT pg_reload_conf()", PG, W, DENY, UNSAFE),
    ("SELECT pg_read_file('/etc/passwd')", PG, W, DENY, UNSAFE),
    ("SELECT setval('s', 1)", PG, W, DENY, UNSAFE),
    ("SELECT pg_notify('c', 'x')", PG, W, DENY, UNSAFE),
    ("SELECT pg_cancel_backend(1)", PG, X, DENY, UNSAFE),
    ("SELECT set_config('statement_timeout', '0', false)", PG, W, DENY, UNSAFE),
    ("SELECT SLEEP(1)", MY, W, DENY, UNSAFE),
    ("SELECT a FROM t ORDER BY SLEEP(1)", MY, W, DENY, UNSAFE),
    ("SELECT RELEASE_ALL_LOCKS()", MY, W, DENY, UNSAFE),
    ("SELECT LOAD_FILE('/etc/passwd')", MY, W, DENY, UNSAFE),
    ("SELECT load_extension('x')", LITE, X, DENY, UNSAFE),

    // R-13: safe_write needs every source and expression to be a read.
    ("INSERT INTO t SELECT pg_terminate_backend(pid) FROM pg_stat_activity", PG, X, DENY, UNSAFE),
    ("INSERT INTO t VALUES (pg_sleep(1))", PG, W, DENY, UNSAFE),
    ("UPDATE t SET a = pg_sleep(1)", PG, W, DENY, UNSAFE),
    ("UPDATE t SET a = (SELECT dblink_exec('c', 'x'))", PG, W, DENY, UNSAFE),
    ("UPDATE t SET a = 1 WHERE id IN (SELECT pg_advisory_lock(1))", PG, W, DENY, UNSAFE),
    ("INSERT INTO t VALUES (1) ON CONFLICT (id) DO UPDATE SET a = pg_advisory_lock(1)", PG, W, DENY, UNSAFE),
    ("INSERT INTO t SELECT * FROM (WITH d AS (DELETE FROM s RETURNING *) SELECT * FROM d) x", PG, W, DENY, UNSAFE),
    ("INSERT INTO t SELECT * FROM s FOR UPDATE", PG, W, DENY, UNSAFE),
    ("MERGE INTO t USING (SELECT pg_sleep(1) AS id) s ON t.id = s.id WHEN MATCHED THEN UPDATE SET a = 1", PG, W, DENY, UNSAFE),
    ("INSERT INTO t VALUES (1)", PG, W, DENY, SAFE),
    ("INSERT INTO t SELECT a FROM s WHERE b > 1", PG, W, DENY, SAFE),
    ("UPDATE t SET a = a + 1 WHERE id IN (SELECT id FROM s)", PG, W, DENY, SAFE),
    ("UPDATE t SET doc = jsonb_set(doc, '{a}', '1') WHERE id = 1", PG, W, DENY, SAFE),
    ("WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x", PG, W, DENY, SAFE),
    ("REPLACE INTO t VALUES (1)", MY, W, DENY, SAFE),
    ("DELETE FROM t", PG, W, DENY, UNSAFE),

    // R-14: only known read pragmas are reads; an argument to a setting writes.
    ("PRAGMA user_version(42)", LITE, W, DENY, UNSAFE),
    ("PRAGMA user_version = 42", LITE, W, DENY, UNSAFE),
    ("PRAGMA writable_schema(1)", LITE, W, DENY, UNSAFE),
    ("PRAGMA journal_mode(DELETE)", LITE, W, DENY, UNSAFE),
    ("PRAGMA journal_mode = WAL", LITE, W, DENY, UNSAFE),
    ("PRAGMA main.journal_mode(WAL)", LITE, W, DENY, UNSAFE),
    ("PRAGMA optimize", LITE, W, DENY, UNSAFE),
    ("PRAGMA wal_checkpoint(TRUNCATE)", LITE, W, DENY, UNSAFE),
    ("PRAGMA incremental_vacuum", LITE, W, DENY, UNSAFE),
    ("PRAGMA user_version", LITE, R, ALLOW, UNSAFE),
    ("PRAGMA journal_mode", LITE, R, ALLOW, UNSAFE),
    ("PRAGMA table_info(t)", LITE, R, ALLOW, UNSAFE),
    ("PRAGMA main.table_info('t')", LITE, R, ALLOW, UNSAFE),
    ("PRAGMA index_list(t)", LITE, R, ALLOW, UNSAFE),
    ("PRAGMA foreign_key_list(t)", LITE, R, ALLOW, UNSAFE),
    ("PRAGMA integrity_check", LITE, R, ALLOW, UNSAFE),

    // B11: harmless session SET runs in Read mode; role, read-only and timeouts do not.
    ("SET search_path TO public", PG, S, ALLOW, UNSAFE),
    ("SET search_path = a, b", PG, S, ALLOW, UNSAFE),
    ("SET LOCAL search_path TO public", PG, S, ALLOW, UNSAFE),
    ("SET SESSION search_path TO public", PG, S, ALLOW, UNSAFE),
    ("SET TIME ZONE 'UTC'", PG, S, ALLOW, UNSAFE),
    ("SET timezone = 'UTC'", PG, S, ALLOW, UNSAFE),
    ("SET application_name = 'mystique'", PG, S, ALLOW, UNSAFE),
    ("SET datestyle TO ISO", PG, S, ALLOW, UNSAFE),
    ("SET search_path TO public; SELECT 1", PG, S, ALLOW, UNSAFE),
    ("SET time_zone = '+00:00'", MY, S, ALLOW, UNSAFE),
    ("SET SESSION time_zone = '+00:00'", MY, S, ALLOW, UNSAFE),
    ("SET @@session.time_zone = '+00:00'", MY, S, ALLOW, UNSAFE),
    ("SET NAMES utf8mb4", MY, S, ALLOW, UNSAFE),
    ("SET @a = 1", MY, S, ALLOW, UNSAFE),
    ("SET @a = (SELECT SLEEP(5))", MY, W, DENY, UNSAFE),
    ("SET statement_timeout = 0", PG, W, DENY, UNSAFE),
    ("SET lock_timeout = 0", PG, W, DENY, UNSAFE),
    ("SET ROLE admin", PG, W, DENY, UNSAFE),
    ("SET SESSION AUTHORIZATION bob", PG, W, DENY, UNSAFE),
    ("SET default_transaction_read_only = off", PG, W, DENY, UNSAFE),
    ("SET transaction_read_only = off", PG, W, DENY, UNSAFE),
    ("SET TRANSACTION READ WRITE", PG, S, DENY, UNSAFE),
    ("SET SESSION CHARACTERISTICS AS TRANSACTION READ WRITE", PG, S, DENY, UNSAFE),
    ("SET standard_conforming_strings = off", PG, S, DENY, UNSAFE),
    ("SET work_mem = '1GB'", PG, S, DENY, UNSAFE),
    ("SET GLOBAL time_zone = '+00:00'", MY, W, DENY, UNSAFE),
    ("SET @@global.time_zone = '+00:00'", MY, W, DENY, UNSAFE),
    ("SET SESSION transaction_read_only = 0", MY, W, DENY, UNSAFE),
    ("SET sql_mode = 'NO_BACKSLASH_ESCAPES'", MY, S, DENY, UNSAFE),
    ("SET PERSIST max_connections = 10", MY, W, DENY, UNSAFE),
    ("RESET search_path", PG, W, DENY, UNSAFE),

    // F-06: ReadProof. Write-mode risk is unchanged, but Read mode only runs
    // functions on the dialect's allowlist.
    ("SELECT my_func(1)", PG, R, DENY, UNSAFE),
    ("SELECT public.lower('x')", PG, R, DENY, UNSAFE),
    ("SELECT * FROM my_srf() f", PG, R, DENY, UNSAFE),
    ("SELECT txid_current()", PG, R, DENY, UNSAFE),
    ("SELECT query_to_xml('DELETE FROM t', true, false, '')", PG, R, DENY, UNSAFE),
    ("SELECT 1; SELECT my_func(1)", PG, R, DENY, UNSAFE),
    ("EXPLAIN ANALYZE SELECT my_func(1)", PG, R, DENY, UNSAFE),
    ("EXPLAIN SELECT my_func(1)", PG, R, ALLOW, UNSAFE),
    ("SELECT my_func(1)", MY, R, DENY, UNSAFE),
    ("SELECT IS_FREE_LOCK('x')", MY, W, DENY, UNSAFE),
    ("EXPLAIN SELECT * FROM t WHERE a = (SELECT GET_LOCK('x', 1))", MY, R, DENY, UNSAFE),
    ("SELECT my_func(1)", LITE, R, DENY, UNSAFE),
    ("SELECT * FROM pragma_table_info('t')", LITE, R, ALLOW, UNSAFE),
    ("-- note\nPRAGMA table_info(t)", LITE, R, ALLOW, UNSAFE),
    ("-- note\nTABLE t", PG, R, ALLOW, UNSAFE),
    ("SELECT pg_catalog.now(), pg_catalog.count(*) FROM t", PG, R, ALLOW, UNSAFE),

    // Ordinary reads stay reads.
    ("SELECT 1", PG, R, ALLOW, UNSAFE),
    ("SELECT * FROM t WHERE a = 1 ORDER BY b DESC NULLS LAST LIMIT 10 OFFSET 5", PG, R, ALLOW, UNSAFE),
    ("WITH c AS (SELECT a FROM t) SELECT * FROM c", PG, R, ALLOW, UNSAFE),
    ("WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 5) SELECT * FROM r", PG, R, ALLOW, UNSAFE),
    ("SELECT data->>'a', data #> '{a,b}', data @> '{\"a\":1}' FROM t", PG, R, ALLOW, UNSAFE),
    ("SELECT jsonb_agg(x) FILTER (WHERE x > 0) FROM t", PG, R, ALLOW, UNSAFE),
    ("SELECT a, row_number() OVER (PARTITION BY b ORDER BY c) FROM t", PG, R, ALLOW, UNSAFE),
    ("SELECT sum(a) OVER w FROM t WINDOW w AS (PARTITION BY b)", PG, R, ALLOW, UNSAFE),
    ("SELECT a, sum(b) FROM t GROUP BY a HAVING sum(b) > 1", PG, R, ALLOW, UNSAFE),
    ("SELECT ARRAY[1, 2, 3], ARRAY(SELECT a FROM t)", PG, R, ALLOW, UNSAFE),
    ("SELECT count(*), max(a), coalesce(b, 0), now(), current_date FROM t", PG, R, ALLOW, UNSAFE),
    ("SELECT * FROM t1 JOIN t2 USING (id) LEFT JOIN t3 ON t3.id = t1.id", PG, R, ALLOW, UNSAFE),
    ("SELECT * FROM unnest(ARRAY[1, 2]) u", PG, R, ALLOW, UNSAFE),
    ("SELECT a::text, CAST(b AS int) FROM t", PG, R, ALLOW, UNSAFE),
    ("SELECT currval('s'), lastval()", PG, R, ALLOW, UNSAFE),
    ("SELECT pg_size_pretty(pg_total_relation_size('t'))", PG, R, ALLOW, UNSAFE),
    ("SELECT 1 UNION SELECT 2", PG, R, ALLOW, UNSAFE),
    ("SELECT * FROM t WHERE EXISTS (SELECT 1 FROM s WHERE s.id = t.id)", PG, R, ALLOW, UNSAFE),
    ("SELECT lower(name) FROM users WHERE name ILIKE '%a%'", PG, R, ALLOW, UNSAFE),
    ("SELECT string_agg(name, ',' ORDER BY name) FROM t", PG, R, ALLOW, UNSAFE),
    ("SELECT * FROM t WHERE ts > now() - interval '1 day'", PG, R, ALLOW, UNSAFE),
    ("SELECT * FROM t LIMIT (SELECT 5)", PG, R, ALLOW, UNSAFE),
    ("TABLE t", PG, R, ALLOW, UNSAFE),
    ("SHOW search_path", PG, R, ALLOW, UNSAFE),
    ("SELECT `a` FROM `t` WHERE b = 'x' LIMIT 5", MY, R, ALLOW, UNSAFE),
    ("SELECT JSON_EXTRACT(doc, '$.a') FROM t", MY, R, ALLOW, UNSAFE),
    ("SHOW TABLES", MY, R, ALLOW, UNSAFE),
    ("SELECT json_extract(doc, '$.a') FROM t", LITE, R, ALLOW, UNSAFE),
    ("SELECT * FROM sqlite_master", LITE, R, ALLOW, UNSAFE),
    ("SELECT * FROM t FOR UPDATE", PG, W, DENY, UNSAFE),
    ("SELECT * INTO t2 FROM t", PG, W, DENY, UNSAFE),
];

#[test]
fn sql_corpus() {
    let mut failures = Vec::new();
    for &(sql, dialect, kind, read_ok, safe) in SQL {
        match gate_sql_batch(sql, dialect, QueryMode::Write) {
            Ok(plan) if plan.worst == kind => {}
            Ok(plan) => failures.push(format!(
                "{sql:?} ({dialect:?}): Write mode kind {:?}, want {kind:?}",
                plan.worst
            )),
            Err(err) => failures.push(format!("{sql:?} ({dialect:?}): Write mode refused: {err}")),
        }
        let read = gate_sql_batch(sql, dialect, QueryMode::Read);
        if read.is_ok() != read_ok {
            failures.push(format!(
                "{sql:?} ({dialect:?}): Read mode {}, want {}",
                if read.is_ok() { "ran" } else { "refused" },
                if read_ok { "ran" } else { "refused" },
            ));
        }
        if sql_is_safe_dml_for(sql, dialect) != safe {
            failures.push(format!(
                "{sql:?} ({dialect:?}): safe_write {}, want {safe}",
                !safe
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

const PG_READS: &str = include_str!("../../mystique-sql/tests/fixtures/read_corpus/postgres.sql");
const MY_READS: &str = include_str!("../../mystique-sql/tests/fixtures/read_corpus/mysql.sql");
const LITE_READS: &str = include_str!("../../mystique-sql/tests/fixtures/read_corpus/sqlite.sql");

/// Real-world reads (the `mystique-sql` ReadProof corpus) still run in Read
/// mode, and the policy's `read_proof` (what MCP calls) agrees.
#[test]
fn real_world_reads_run_in_read_mode() {
    let corpora = [
        (PG, "postgres", PG_READS),
        (MY, "mysql", MY_READS),
        (LITE, "sqlite", LITE_READS),
    ];
    let mut failures = Vec::new();
    let mut count = 0;
    for (dialect, engine, corpus) in corpora {
        let policy = SqlQueryPolicy::new(engine);
        let masked = mystique_sql::mask_placeholders(corpus, dialect);
        for span in mystique_sql::split_statement_spans(&masked, dialect) {
            count += 1;
            let sql = &corpus[span.start..span.end];
            let script = mystique_sql::ScriptParams::scan(sql, dialect).expect("scan");
            let wanted = (0..10)
                .find(|n| script.check_count(*n).is_ok())
                .unwrap_or(0);
            let params = vec![serde_json::Value::from(1); wanted];
            if let Err(err) = gate_sql_batch_in(sql, dialect, QueryMode::Read, false, &params) {
                failures.push(format!("{dialect:?}: {err} <- {sql}"));
            }
            // MCP sends no parameters.
            if wanted == 0 {
                if let Err(err) = policy.read_proof(sql) {
                    failures.push(format!("{dialect:?} read_proof: {err} <- {sql}"));
                }
            }
        }
    }
    assert!(count >= 200, "corpus has {count} statements");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// MCP and the agent call one function; the refusal is a positioned policy error.
#[test]
fn read_proof_is_one_call_for_mcp() {
    let pg = SqlQueryPolicy::new("postgres");
    assert!(pg.read_proof("SELECT lower(name) FROM users").is_ok());
    let err = pg.read_proof("SELECT nextval('s')").expect_err("nextval");
    assert_eq!(
        err.to_string(),
        "policy: `nextval` may write; switch to Write mode to run it. Nothing ran."
    );
    let ipc = IpcError::from(err);
    assert_eq!(ipc.kind, IpcErrorKind::Policy);
    assert_eq!(
        ipc.message,
        "`nextval` may write; switch to Write mode to run it. Nothing ran."
    );
    assert_eq!(ipc.position.expect("position").offset, 7);
    assert_eq!(ipc.statement_index, Some(0));

    let err = pg
        .read_proof("SELECT 1;\nSELECT my_func(2)")
        .expect_err("udf");
    let ipc = IpcError::from(err);
    assert_eq!(
        ipc.message,
        "Statement 2: `my_func` is not a known read-only function; switch to Write mode to run it. Nothing ran."
    );
    let position = ipc.position.expect("position");
    assert_eq!((position.line, position.column), (2, 8));
    assert_eq!(ipc.statement_index, Some(1));

    assert!(pg.read_proof("DELETE FROM t").is_err());
    assert!(pg.read_proof("BEGIN").is_err());
    let mysql = SqlQueryPolicy::new("mysql");
    assert!(mysql.read_proof("SELECT GET_LOCK('x', 1)").is_err());
    assert!(mysql
        .read_proof("SELECT JSON_EXTRACT(doc, '$.a') FROM t")
        .is_ok());
}
