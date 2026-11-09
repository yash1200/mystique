//! Fail-closed ReadProof (F-06): real-world reads pass, side effects do not.

use mystique_sql::{
    classify, is_read_safe_set, mask_placeholders, read_proof, split_statement_spans, DenyKind,
    ReadProofReason, SqlDialect, StatementRisk,
};

const PG: SqlDialect = SqlDialect::Postgres;
const MY: SqlDialect = SqlDialect::Mysql;
const LITE: SqlDialect = SqlDialect::Sqlite;

const PG_CORPUS: &str = include_str!("fixtures/read_corpus/postgres.sql");
const MY_CORPUS: &str = include_str!("fixtures/read_corpus/mysql.sql");
const LITE_CORPUS: &str = include_str!("fixtures/read_corpus/sqlite.sql");

fn corpus_failures(dialect: SqlDialect, corpus: &str) -> (usize, Vec<String>) {
    let masked = mask_placeholders(corpus, dialect);
    let spans = split_statement_spans(&masked, dialect);
    let mut failures = Vec::new();
    for span in &spans {
        let sql = &corpus[span.start..span.end];
        // Each entry is a read today; the corpus is what ReadProof must keep.
        let read_today = classify(&span.text, dialect) == StatementRisk::Read
            || is_read_safe_set(&span.text, dialect);
        if !read_today {
            failures.push(format!("not a read before ReadProof: {sql}"));
        }
        if let Err(err) = read_proof(dialect, sql) {
            failures.push(format!("{err} <- {sql}"));
        }
    }
    (spans.len(), failures)
}

#[test]
fn real_world_reads_pass() {
    let mut total = 0;
    let mut failures = Vec::new();
    for (dialect, corpus) in [(PG, PG_CORPUS), (MY, MY_CORPUS), (LITE, LITE_CORPUS)] {
        let (count, mut bad) = corpus_failures(dialect, corpus);
        assert!(count > 40, "{dialect:?} corpus has {count} statements");
        total += count;
        failures.append(&mut bad);
    }
    assert!(total >= 200, "corpus has {total} statements");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

fn reason(dialect: SqlDialect, sql: &str) -> ReadProofReason {
    read_proof(dialect, sql).expect_err(sql).reason
}

fn function(name: &str, kind: DenyKind) -> ReadProofReason {
    ReadProofReason::Function {
        name: name.into(),
        kind,
    }
}

fn unknown(name: &str) -> ReadProofReason {
    ReadProofReason::UnknownFunction { name: name.into() }
}

#[test]
fn postgres_side_effect_functions_name_their_reason() {
    let cases = [
        ("SELECT nextval('s')", function("nextval", DenyKind::Writes)),
        (
            "SELECT setval('s', 1)",
            function("setval", DenyKind::Writes),
        ),
        (
            "SELECT txid_current()",
            function("txid_current", DenyKind::Writes),
        ),
        (
            "SELECT pg_current_xact_id()",
            function("pg_current_xact_id", DenyKind::Writes),
        ),
        (
            "SELECT lo_import('/x')",
            function("lo_import", DenyKind::Writes),
        ),
        (
            "SELECT pg_notify('c', 'x')",
            function("pg_notify", DenyKind::Writes),
        ),
        ("SELECT pg_sleep(1)", function("pg_sleep", DenyKind::Waits)),
        (
            "SELECT pg_catalog.pg_sleep(1)",
            function("pg_catalog.pg_sleep", DenyKind::Waits),
        ),
        (
            "SELECT pg_advisory_lock(1)",
            function("pg_advisory_lock", DenyKind::Locks),
        ),
        (
            "SELECT pg_try_advisory_xact_lock(1)",
            function("pg_try_advisory_xact_lock", DenyKind::Locks),
        ),
        (
            "SELECT pg_read_file('/etc/passwd')",
            function("pg_read_file", DenyKind::ServerFiles),
        ),
        (
            "SELECT * FROM pg_ls_dir('.')",
            function("pg_ls_dir", DenyKind::ServerFiles),
        ),
        (
            "SELECT set_config('statement_timeout', '0', false)",
            function("set_config", DenyKind::Settings),
        ),
        (
            "SELECT * FROM dblink('c', 'DELETE FROM t') AS x(a int)",
            function("dblink", DenyKind::OtherConnection),
        ),
        (
            "SELECT 1 ORDER BY (SELECT dblink_exec('c', 'DELETE FROM t'))",
            function("dblink_exec", DenyKind::OtherConnection),
        ),
        (
            "SELECT public.dblink_exec('c', 'x')",
            function("public.dblink_exec", DenyKind::OtherConnection),
        ),
        (
            "SELECT query_to_xml('DELETE FROM t', true, false, '')",
            function("query_to_xml", DenyKind::RunsSql),
        ),
        (
            "SELECT * FROM ts_stat('SELECT v FROM t')",
            function("ts_stat", DenyKind::RunsSql),
        ),
        (
            "SELECT pg_terminate_backend(1)",
            function("pg_terminate_backend", DenyKind::Admin),
        ),
        (
            "SELECT pg_reload_conf()",
            function("pg_reload_conf", DenyKind::Admin),
        ),
    ];
    for (sql, want) in cases {
        assert_eq!(reason(PG, sql), want, "{sql}");
    }
}

#[test]
fn mysql_side_effect_functions_name_their_reason() {
    let cases = [
        (
            "SELECT GET_LOCK('x', 1)",
            function("GET_LOCK", DenyKind::Locks),
        ),
        (
            "SELECT RELEASE_LOCK('x')",
            function("RELEASE_LOCK", DenyKind::Locks),
        ),
        (
            "SELECT IS_FREE_LOCK('x')",
            function("IS_FREE_LOCK", DenyKind::Locks),
        ),
        ("SELECT SLEEP(1)", function("SLEEP", DenyKind::Waits)),
        (
            "SELECT BENCHMARK(1000000, MD5('x'))",
            function("BENCHMARK", DenyKind::Waits),
        ),
        (
            "SELECT LOAD_FILE('/etc/passwd')",
            function("LOAD_FILE", DenyKind::ServerFiles),
        ),
        (
            "SELECT a FROM t ORDER BY SLEEP(1)",
            function("SLEEP", DenyKind::Waits),
        ),
        ("SELECT NEXTVAL(s)", function("NEXTVAL", DenyKind::Writes)),
    ];
    for (sql, want) in cases {
        assert_eq!(reason(MY, sql), want, "{sql}");
    }
}

#[test]
fn sqlite_side_effect_functions_name_their_reason() {
    assert_eq!(
        reason(LITE, "SELECT load_extension('x')"),
        function("load_extension", DenyKind::LoadsCode)
    );
    assert_eq!(
        reason(LITE, "SELECT writefile('/tmp/x', 'y')"),
        function("writefile", DenyKind::ServerFiles)
    );
    assert_eq!(
        reason(LITE, "SELECT readfile('/etc/passwd')"),
        function("readfile", DenyKind::ServerFiles)
    );
}

#[test]
fn unknown_and_user_functions_fail_closed() {
    assert_eq!(reason(PG, "SELECT my_func(1)"), unknown("my_func"));
    assert_eq!(
        reason(PG, "SELECT public.lower('x')"),
        unknown("public.lower")
    );
    assert_eq!(reason(PG, "SELECT \"LOWER\"('x')"), unknown("\"LOWER\""));
    assert_eq!(reason(PG, "SELECT * FROM my_srf(1) f"), unknown("my_srf"));
    assert_eq!(
        reason(PG, "SELECT a FROM t WHERE a = ANY(ARRAY[my_func(1)])"),
        unknown("my_func")
    );
    assert_eq!(
        reason(
            PG,
            "SELECT sum(a) OVER w FROM t WINDOW w AS (PARTITION BY f(a))"
        ),
        unknown("f")
    );
    assert_eq!(
        reason(MY, "SELECT mydb.my_func(1)"),
        unknown("mydb.my_func")
    );
    assert_eq!(reason(MY, "SELECT my_func(1)"), unknown("my_func"));
    assert_eq!(reason(LITE, "SELECT my_func(1)"), unknown("my_func"));
    // An engine's functions are not another engine's.
    assert_eq!(
        reason(MY, "SELECT generate_series(1, 2)"),
        unknown("generate_series")
    );
    assert_eq!(
        reason(LITE, "SELECT DATE_FORMAT(d, '%Y')"),
        unknown("DATE_FORMAT")
    );
    // Generic (unknown engine) only knows standard SQL.
    assert_eq!(
        reason(SqlDialect::Generic, "SELECT jsonb_build_object('a', 1)"),
        unknown("jsonb_build_object")
    );
    assert!(read_proof(SqlDialect::Generic, "SELECT count(*), lower(a) FROM t").is_ok());
}

#[test]
fn write_constructs_inside_a_read_fail() {
    let lock = |clause: &str| ReadProofReason::RowLock(clause.into());
    assert_eq!(reason(PG, "SELECT * FROM t FOR UPDATE"), lock("FOR UPDATE"));
    assert_eq!(reason(PG, "SELECT * FROM t FOR SHARE"), lock("FOR SHARE"));
    assert_eq!(
        reason(PG, "SELECT * FROM (SELECT * FROM t FOR UPDATE) s"),
        lock("FOR UPDATE")
    );
    assert_eq!(reason(MY, "SELECT * FROM t FOR UPDATE"), lock("FOR UPDATE"));
    assert_eq!(
        reason(PG, "SELECT * INTO new_table FROM t"),
        ReadProofReason::SelectInto("new_table".into())
    );
    let nested = |kind: &str| ReadProofReason::NestedWrite(kind.into());
    assert_eq!(
        reason(PG, "WITH x AS (DELETE FROM t RETURNING *) SELECT * FROM x"),
        nested("DELETE")
    );
    assert_eq!(
        reason(
            PG,
            "WITH x AS (UPDATE t SET a = 1 RETURNING *) SELECT * FROM x"
        ),
        nested("UPDATE")
    );
    assert_eq!(
        reason(
            PG,
            "WITH x AS (INSERT INTO t VALUES (1) RETURNING *) SELECT * FROM x"
        ),
        nested("INSERT")
    );
    assert_eq!(
        reason(PG, "SELECT 1 UNION ALL SELECT * FROM (WITH d AS (DELETE FROM t RETURNING id) SELECT * FROM d) s"),
        nested("DELETE")
    );
}

#[test]
fn statements_that_are_not_reads_fail() {
    let not = |kind: &str| ReadProofReason::NotARead(kind.into());
    assert_eq!(reason(PG, "DELETE FROM t"), not("DELETE"));
    assert_eq!(
        reason(PG, "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x"),
        ReadProofReason::NestedWrite("INSERT".into())
    );
    assert_eq!(
        reason(PG, "EXPLAIN ANALYZE DELETE FROM t"),
        not("EXPLAIN ANALYZE DELETE")
    );
    assert_eq!(reason(PG, "DO $$ BEGIN DROP TABLE t; END $$"), not("DO"));
    assert_eq!(reason(PG, "SET ROLE admin"), not("SET"));
    assert_eq!(reason(PG, "SET statement_timeout = 0"), not("SET"));
    assert_eq!(reason(PG, "BEGIN"), not("BEGIN"));
    assert_eq!(reason(LITE, "ATTACH DATABASE 'x.db' AS x"), not("ATTACH"));
    assert_eq!(reason(LITE, "PRAGMA user_version = 42"), not("PRAGMA"));
    assert_eq!(
        reason(LITE, "PRAGMA journal_mode(WAL)"),
        ReadProofReason::Unparsed
    );
    assert_eq!(reason(MY, "CALL p()"), not("CALL"));
    assert_eq!(
        reason(MY, "SELECT 1 /*!50000 , 1 */"),
        ReadProofReason::VersionedComment
    );
    assert_eq!(
        reason(MY, "SELECT * FROM t INTO OUTFILE '/tmp/x'"),
        ReadProofReason::Unparsed
    );
    assert_eq!(reason(PG, "VACUUM"), ReadProofReason::Unparsed);
    assert_eq!(reason(PG, "SELECT * FORM t"), ReadProofReason::Unparsed);
    // One bad statement fails the batch, and says which.
    let err = read_proof(PG, "SELECT 1; SELECT 2; SELECT nextval('s')").unwrap_err();
    assert_eq!(err.statement_index, 2);
}

#[test]
fn explain_follows_what_actually_runs() {
    // Postgres and SQLite plan without calling anything.
    assert!(read_proof(PG, "EXPLAIN SELECT pg_sleep(1)").is_ok());
    assert!(read_proof(PG, "EXPLAIN DELETE FROM t").is_ok());
    assert!(read_proof(LITE, "EXPLAIN QUERY PLAN SELECT my_func(1)").is_ok());
    // ANALYZE runs the statement, so the statement must be a provable read.
    assert_eq!(
        reason(PG, "EXPLAIN ANALYZE SELECT nextval('s')"),
        function("nextval", DenyKind::Writes)
    );
    assert!(read_proof(PG, "EXPLAIN (ANALYZE) SELECT 1").is_ok());
    // MySQL evaluates constant subqueries while it plans.
    assert_eq!(
        reason(
            MY,
            "EXPLAIN SELECT * FROM t WHERE a = (SELECT GET_LOCK('x', 1))"
        ),
        function("GET_LOCK", DenyKind::Locks)
    );
    assert!(read_proof(MY, "EXPLAIN SELECT * FROM t WHERE a = 1").is_ok());
}

#[test]
fn harmless_session_set_still_reads_but_its_value_is_checked() {
    assert!(read_proof(PG, "SET search_path TO public").is_ok());
    assert!(read_proof(MY, "SET @a = 1").is_ok());
    assert_eq!(
        reason(MY, "SET @a = (SELECT my_func())"),
        unknown("my_func")
    );
}
