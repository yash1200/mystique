use super::*;

fn t(database: &str, schema: Option<&str>) -> WriteTarget {
    WriteTarget {
        database: database.into(),
        schema: schema.map(str::to_string),
    }
}

fn pg(sql: &str) -> WriteTargets {
    write_targets(sql, SqlDialect::Postgres, Some("app"))
}

fn my(sql: &str) -> WriteTargets {
    write_targets(sql, SqlDialect::Mysql, Some("app"))
}

fn named(targets: Vec<WriteTarget>) -> WriteTargets {
    WriteTargets {
        targets,
        unresolved: false,
    }
}

#[test]
fn reads_name_nothing() {
    assert!(pg("SELECT * FROM prod.public.t").is_empty());
    assert!(my("SELECT * FROM prod.t; SHOW TABLES").is_empty());
    assert!(pg("").is_empty());
}

#[test]
fn postgres_unqualified_and_schema_names_are_the_current_database() {
    assert_eq!(pg("UPDATE t SET a = 1"), named(vec![t("app", None)]));
    assert_eq!(
        pg("DELETE FROM sales.orders WHERE id = 1"),
        named(vec![t("app", Some("sales"))])
    );
}

#[test]
fn postgres_three_part_name_is_that_database() {
    assert_eq!(
        pg("UPDATE prod.public.users SET a = 1"),
        named(vec![t("prod", Some("public"))])
    );
}

#[test]
fn postgres_folds_unquoted_names_and_keeps_quoted_ones() {
    assert_eq!(
        pg("INSERT INTO Prod.Public.T VALUES (1)"),
        named(vec![t("prod", Some("public"))])
    );
    assert_eq!(
        pg(r#"INSERT INTO "Prod"."Sales".t VALUES (1)"#),
        named(vec![t("Prod", Some("Sales"))])
    );
}

#[test]
fn mysql_two_part_name_is_a_database() {
    assert_eq!(my("UPDATE prod.t SET a = 1"), named(vec![t("prod", None)]));
    assert_eq!(
        my("INSERT INTO `Prod DB`.`t` VALUES (1)"),
        named(vec![t("Prod DB", None)])
    );
    assert_eq!(
        my("DELETE FROM t WHERE id = 1"),
        named(vec![t("app", None)])
    );
}

#[test]
fn mysql_use_moves_the_statements_after_it() {
    assert_eq!(
        my("UPDATE t SET a = 1; USE prod; UPDATE t SET a = 2"),
        named(vec![t("app", None), t("prod", None)])
    );
    assert_eq!(
        write_targets("USE prod; DELETE FROM t", SqlDialect::Mysql, None),
        named(vec![t("prod", None)])
    );
}

#[test]
fn mysql_multi_table_writes_name_every_joined_table() {
    assert_eq!(
        my("UPDATE a JOIN prod.b ON a.id = b.id SET a.x = b.x"),
        named(vec![t("app", None), t("prod", None)])
    );
    assert_eq!(
        my("DELETE a, b FROM a JOIN prod.b ON a.id = b.id"),
        named(vec![t("app", None), t("prod", None)])
    );
}

#[test]
fn update_from_reads_do_not_count() {
    assert_eq!(
        pg("UPDATE t SET a = o.a FROM other.o WHERE o.id = t.id"),
        named(vec![t("app", None)])
    );
    assert_eq!(
        my("INSERT INTO scratch.t SELECT * FROM prod.t"),
        named(vec![t("scratch", None)])
    );
}

#[test]
fn ddl_names_its_database() {
    assert_eq!(my("DROP DATABASE prod"), named(vec![t("prod", None)]));
    assert_eq!(my("CREATE SCHEMA prod"), named(vec![t("prod", None)]));
    assert_eq!(
        pg("DROP SCHEMA sales"),
        named(vec![t("app", Some("sales"))])
    );
    assert_eq!(pg("CREATE DATABASE prod"), named(vec![t("prod", None)]));
    assert_eq!(my("TRUNCATE TABLE prod.logs"), named(vec![t("prod", None)]));
    assert_eq!(
        my("DROP TABLE scratch.a, prod.b"),
        named(vec![t("prod", None), t("scratch", None)])
    );
    assert_eq!(
        my("CREATE INDEX i ON prod.t (a)"),
        named(vec![t("prod", None)])
    );
    assert_eq!(
        my("ALTER TABLE prod.t ADD COLUMN b INT"),
        named(vec![t("prod", None)])
    );
    assert_eq!(
        my("RENAME TABLE scratch.t TO prod.t"),
        named(vec![t("prod", None), t("scratch", None)])
    );
    assert_eq!(
        pg("CREATE TABLE sales.t (id int)"),
        named(vec![t("app", Some("sales"))])
    );
}

#[test]
fn data_modifying_cte_names_its_writes() {
    assert_eq!(
        pg("WITH d AS (DELETE FROM sales.a RETURNING *) INSERT INTO b SELECT * FROM d"),
        named(vec![t("app", None), t("app", Some("sales"))])
    );
}

#[test]
fn hidden_and_unparsed_writes_are_unresolved() {
    assert!(pg("SELECT nextval('s')").unresolved);
    assert!(my("CALL app.sync()").unresolved);
    assert!(pg("DO $$ BEGIN DELETE FROM t; END $$").unresolved);
    assert!(pg("UPDATE t SET").unresolved);
    assert!(my("GRANT ALL ON *.* TO x").unresolved);
}

#[test]
fn no_current_database_leaves_unqualified_names_unresolved() {
    let got = write_targets("UPDATE t SET a = 1", SqlDialect::Mysql, None);
    assert!(got.unresolved);
    let got = write_targets(
        "UPDATE t SET a = 1; UPDATE prod.t SET a = 1",
        SqlDialect::Mysql,
        Some("  "),
    );
    assert!(got.unresolved);
    assert_eq!(got.targets, vec![t("prod", None)]);
}

#[test]
fn too_many_parts_are_unresolved() {
    assert!(my("UPDATE a.b.c SET x = 1").unresolved);
    assert!(pg("UPDATE a.b.c.d SET x = 1").unresolved);
}

#[test]
fn sqlite_is_one_database() {
    let got = write_targets(
        "UPDATE aux.t SET a = 1; DELETE FROM main.t",
        SqlDialect::Sqlite,
        None,
    );
    assert_eq!(got, named(vec![t("main", None)]));
}

#[test]
fn session_statements_are_skipped() {
    assert!(my("SET NAMES utf8mb4").is_empty());
    assert!(pg("BEGIN").is_empty());
}

#[test]
fn helpers_build_and_merge() {
    assert!(WriteTargets::current(None).unresolved);
    assert_eq!(
        WriteTargets::current(Some(" app ")),
        named(vec![t("app", None)])
    );
    let mut all = WriteTargets::database("b", None);
    all.extend(WriteTargets::database("a", Some("s")));
    all.extend(WriteTargets::database("b", None));
    assert_eq!(all, named(vec![t("a", Some("s")), t("b", None)]));
    assert!(WriteTargets::database("", None).unresolved);
}
