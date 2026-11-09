//! Structure apply on real SQLite files: plan from a described draft, apply,
//! read the schema back, and compare.

use std::path::PathBuf;

use super::*;
use mystique_sql::ddl::{plan_table_alter, DdlTarget};

/// A database file in the temp dir, removed on drop.
struct TempDb {
    path: PathBuf,
    conn: Option<Connection>,
}

impl TempDb {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "mystique-{tag}-{}-{}.sqlite",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let conn = Connection::open(&path).unwrap_or_else(|err| panic!("{err}"));
        Self {
            path,
            conn: Some(conn),
        }
    }

    fn conn(&self) -> &Connection {
        self.conn.as_ref().unwrap_or_else(|| panic!("closed"))
    }

    fn run(&self, sql: &str) {
        self.conn()
            .execute_batch(sql)
            .unwrap_or_else(|err| panic!("{err}: {sql}"));
    }

    fn describe(&self, table: &str) -> TableDefinition {
        describe_on(self.conn(), table).unwrap_or_else(|err| panic!("{err}"))
    }

    /// Plan and apply; the plan must be applyable.
    fn apply(&self, draft: &TableDefinition) -> (DdlPlan, ApplyDdlResult) {
        let plan = plan_table_alter(DdlTarget::SQLITE, draft);
        assert!(plan.applyable(), "{:?}", plan.warnings);
        let result = apply_on(self.conn(), &plan, draft).unwrap_or_else(|err| {
            panic!("{err}\n{}", plan.statements.join(";\n"));
        });
        (plan, result)
    }

    /// SQL of every table, index, trigger, and view, by name.
    fn schema(&self) -> Vec<(String, String)> {
        let mut stmt = self
            .conn()
            .prepare("SELECT name, sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name")
            .unwrap_or_else(|err| panic!("{err}"));
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap_or_else(|err| panic!("{err}"));
        rows.map(|row| row.unwrap_or_else(|err| panic!("{err}")))
            .collect()
    }

    fn sql_of(&self, name: &str) -> String {
        self.schema()
            .into_iter()
            .find(|(item, _)| item == name)
            .map(|(_, sql)| sql)
            .unwrap_or_else(|| panic!("{name} is gone: {:?}", self.schema()))
    }

    fn rows(&self, sql: &str) -> Vec<Vec<String>> {
        let mut stmt = self
            .conn()
            .prepare(sql)
            .unwrap_or_else(|err| panic!("{err}"));
        let width = stmt.column_count();
        let rows = stmt
            .query_map([], |row| {
                (0..width)
                    .map(|at| {
                        let value: rusqlite::types::Value = row.get(at)?;
                        Ok(match value {
                            rusqlite::types::Value::Null => "NULL".to_string(),
                            rusqlite::types::Value::Integer(v) => v.to_string(),
                            rusqlite::types::Value::Real(v) => v.to_string(),
                            rusqlite::types::Value::Text(v) => v,
                            rusqlite::types::Value::Blob(v) => format!("{v:?}"),
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_else(|err| panic!("{err}"));
        rows.map(|row| row.unwrap_or_else(|err| panic!("{err}")))
            .collect()
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        drop(self.conn.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

fn column<'a>(draft: &'a mut TableDefinition, name: &str) -> &'a mut ColumnDraft {
    draft
        .columns
        .iter_mut()
        .find(|column| column.name == name)
        .unwrap_or_else(|| panic!("no column {name}"))
}

fn assert_contains(haystack: &str, needles: &[&str]) {
    for needle in needles {
        assert!(haystack.contains(needle), "lost {needle}: {haystack}");
    }
}

const TRICKY: &str = "
CREATE TABLE p (id INTEGER PRIMARY KEY, code TEXT UNIQUE);
INSERT INTO p VALUES (1, 'a'), (2, 'b');
CREATE TABLE t (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    a TEXT DEFAULT 'x' COLLATE NOCASE CHECK (a <> ''),
    b INTEGER UNIQUE,
    loose,
    pid INTEGER REFERENCES p(id) ON DELETE CASCADE,
    c TEXT,
    total INTEGER GENERATED ALWAYS AS (b * 2) VIRTUAL,
    CONSTRAINT t_b_positive CHECK (b > 0)
);
CREATE INDEX t_c ON t(c);
CREATE INDEX t_lower_a ON t(lower(a)) WHERE b > 1;
CREATE TRIGGER t_trg AFTER INSERT ON t BEGIN SELECT 1; END;
CREATE VIEW t_view AS SELECT id, a, b FROM t;
INSERT INTO t (a, b, loose, pid, c) VALUES ('y', 1, 'free', 1, 'z'), ('w', 2, 7, 2, 'v');
DELETE FROM t WHERE id = 2;
INSERT INTO t (a, b, loose, pid, c) VALUES ('q', 3, NULL, 1, 'u');
";

#[test]
fn add_nullable_text_column_matches_the_preview() {
    let db = TempDb::new("add");
    db.run("CREATE TABLE orders (id INTEGER PRIMARY KEY)");
    let mut draft = db.describe("orders");
    draft.columns.push(ColumnDraft {
        name: "note".into(),
        data_type: "text".into(),
        nullable: true,
        ..ColumnDraft::default()
    });
    let (plan, applied) = db.apply(&draft);
    assert!(!plan.requires_rebuild);
    assert_eq!(applied.applied, plan.statements);
    let again = db.describe("orders");
    assert!(again.columns.iter().any(|column| {
        column.name == "note" && column.nullable && column.data_type.eq_ignore_ascii_case("text")
    }));
}

#[test]
fn describe_reads_constraints_defaults_and_keeps_untyped_columns_untyped() {
    let db = TempDb::new("describe");
    db.run(TRICKY);
    let draft = db.describe("t");
    let loose = draft
        .columns
        .iter()
        .find(|c| c.name == "loose")
        .unwrap_or_else(|| panic!("loose"));
    assert_eq!(loose.data_type, "", "untyped stays untyped, not ANY");
    let id = &draft.columns[0];
    assert_eq!(id.identity.as_deref(), Some("autoincrement"));
    let a = &draft.columns[1];
    assert_eq!(a.default_value.as_deref(), Some("'x'"));
    let checks: Vec<_> = draft
        .constraints
        .iter()
        .filter(|c| c.kind == "check")
        .map(|c| (c.name.as_str(), c.expression.as_deref().unwrap_or("")))
        .collect();
    assert_eq!(checks, [("", "a <> ''"), ("t_b_positive", "b > 0")]);
    assert!(draft
        .constraints
        .iter()
        .any(|c| c.kind == "unique" && c.columns == ["b"]));
    let fk = &draft.foreign_keys[0];
    assert_eq!(fk.columns, ["pid"]);
    assert_eq!(fk.ref_columns, ["id"]);
    assert_eq!(fk.on_delete.as_deref(), Some("CASCADE"));
    let partial = draft
        .indexes
        .iter()
        .find(|i| i.name == "t_lower_a")
        .unwrap_or_else(|| panic!("t_lower_a"));
    assert!(partial
        .definition
        .as_deref()
        .is_some_and(|sql| sql.contains("WHERE b > 1")));
    assert_eq!(draft.triggers.len(), 1);
}

#[test]
fn a_rebuild_keeps_everything_the_draft_does_not_change() {
    let db = TempDb::new("rebuild");
    db.run(TRICKY);
    let seq_before = db.rows("SELECT seq FROM sqlite_sequence WHERE name = 't'");
    let fk_before = db.rows("PRAGMA foreign_keys")[0][0].clone();
    let before = db.schema();

    // Type change, NOT NULL, and a rename in one go.
    let mut draft = db.describe("t");
    column(&mut draft, "c").data_type = "VARCHAR(20)".into();
    column(&mut draft, "c").nullable = false;
    column(&mut draft, "b").name = "bee".into();
    let (plan, applied) = db.apply(&draft);
    assert!(plan.requires_rebuild);
    assert!(plan.transactional);
    // Foreign keys are turned back on only when they were on before.
    assert_eq!(db.rows("PRAGMA foreign_keys"), [[fk_before.as_str()]]);
    let mut expected = plan.statements.clone();
    if fk_before == "0" {
        expected.retain(|sql| sql != "PRAGMA foreign_keys = ON");
    }
    assert_eq!(applied.applied, expected);

    let table = db.sql_of("t");
    assert_contains(
        &table,
        &[
            "INTEGER PRIMARY KEY AUTOINCREMENT",
            "DEFAULT 'x' COLLATE NOCASE CHECK (a <> '')",
            "\"bee\" INTEGER UNIQUE",
            "loose,",
            "REFERENCES p(id) ON DELETE CASCADE",
            "c VARCHAR(20) NOT NULL",
            "GENERATED ALWAYS AS (\"bee\" * 2) VIRTUAL",
            "CONSTRAINT t_b_positive CHECK (\"bee\" > 0)",
        ],
    );
    assert!(!table.contains("ANY"), "{table}");
    assert_contains(&db.sql_of("t_c"), &["ON t(c)"]);
    assert_contains(&db.sql_of("t_lower_a"), &["lower(a)", "WHERE \"bee\" > 1"]);
    assert_contains(&db.sql_of("t_trg"), &["AFTER INSERT ON t"]);
    // Views were not touched and still compile.
    assert_eq!(
        before
            .iter()
            .find(|(n, _)| n == "t_view")
            .map(|(_, s)| s.contains("b")),
        Some(true)
    );
    assert_eq!(db.rows("SELECT count(*) FROM t_view"), [["2"]]);

    // Rows, rowids, and the AUTOINCREMENT high-water mark survive.
    assert_eq!(
        db.rows("SELECT id, a, bee, loose, pid, c, total FROM t ORDER BY id"),
        [
            ["1", "y", "1", "free", "1", "z", "2"],
            ["3", "q", "3", "NULL", "1", "u", "6"],
        ]
    );
    assert_eq!(
        db.rows("SELECT typeof(loose) FROM t ORDER BY id"),
        [["text"], ["null"]]
    );
    assert_eq!(
        db.rows("SELECT seq FROM sqlite_sequence WHERE name = 't'"),
        seq_before
    );
    db.run("INSERT INTO t (a, bee, pid, c) VALUES ('n', 5, 2, 'k')");
    assert_eq!(db.rows("SELECT max(id) FROM t"), [["4"]]);
    assert!(db
        .rows("SELECT name FROM sqlite_master WHERE name LIKE 'mystique_rebuild%'")
        .is_empty());

    // Read back: the new definition matches the draft.
    let again = db.describe("t");
    let c = again
        .columns
        .iter()
        .find(|c| c.name == "c")
        .map(|c| (c.data_type.clone(), c.nullable));
    assert_eq!(c, Some(("VARCHAR(20)".into(), false)));
    assert!(again.columns.iter().any(|c| c.name == "bee"));
    assert_eq!(again.indexes.len(), 2);
    assert_eq!(again.triggers.len(), 1);
    assert_eq!(again.constraints.len(), draft.constraints.len());
    assert_eq!(again.foreign_keys.len(), 1);
}

#[test]
fn dropping_a_column_removes_what_uses_it_and_says_so() {
    let db = TempDb::new("drop");
    db.run(TRICKY);
    db.run("CREATE TRIGGER t_c_trg AFTER UPDATE OF c ON t BEGIN SELECT NEW.c; END;");
    let mut draft = db.describe("t");
    column(&mut draft, "c").drop = true;
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    let note = plan.note.clone().unwrap_or_default();
    assert_contains(
        &note,
        &[
            "Index t_c uses dropped column c",
            "Trigger t_c_trg uses dropped column c",
        ],
    );
    db.apply(&draft);
    let names: Vec<String> = db.schema().into_iter().map(|(name, _)| name).collect();
    assert!(!names.contains(&"t_c".to_string()), "{names:?}");
    assert!(!names.contains(&"t_c_trg".to_string()), "{names:?}");
    assert!(names.contains(&"t_lower_a".to_string()), "{names:?}");
    assert!(names.contains(&"t_trg".to_string()), "{names:?}");
    assert!(!db.sql_of("t").contains(" c TEXT"));
    assert_eq!(db.rows("SELECT count(*) FROM t"), [["2"]]);
}

#[test]
fn a_foreign_key_violation_rolls_the_rebuild_back() {
    let db = TempDb::new("fkcheck");
    db.run(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE parent (id INTEGER PRIMARY KEY);
         INSERT INTO parent VALUES (1);
         CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER);
         INSERT INTO child VALUES (1, 1), (2, 99);",
    );
    let before = db.schema();
    let mut draft = db.describe("child");
    draft.foreign_keys.push(ForeignKeyDraft {
        name: "child_parent".into(),
        columns: vec!["parent_id".into()],
        ref_table: "parent".into(),
        ref_columns: vec!["id".into()],
        ..ForeignKeyDraft::default()
    });
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    let err = match apply_on(db.conn(), &plan, &draft) {
        Ok(result) => panic!("applied: {result:?}"),
        Err(err) => err.to_string(),
    };
    assert!(err.contains("breaks foreign keys"), "{err}");
    assert!(err.contains("rolled back"), "{err}");
    assert_eq!(db.schema(), before);
    assert_eq!(db.rows("PRAGMA foreign_keys"), [["1"]]);

    // With the bad row gone, the same draft applies and the key is written.
    db.run("DELETE FROM child WHERE id = 2");
    db.apply(&draft);
    assert_contains(
        &db.sql_of("child"),
        &["CONSTRAINT \"child_parent\" FOREIGN KEY (\"parent_id\") REFERENCES \"parent\" (\"id\")"],
    );
    assert_eq!(db.rows("PRAGMA foreign_keys"), [["1"]]);
    let again = db.describe("child");
    assert_eq!(again.foreign_keys.len(), 1);
    assert_eq!(again.foreign_keys[0].name, "child_parent");
}

#[test]
fn a_changed_foreign_key_is_rewritten_and_a_dropped_one_removed() {
    let db = TempDb::new("fkedit");
    db.run(
        "CREATE TABLE a (id INTEGER PRIMARY KEY);
         CREATE TABLE b (id INTEGER PRIMARY KEY);
         CREATE TABLE c (
             id INTEGER PRIMARY KEY,
             a_id INTEGER REFERENCES a(id),
             b_id INTEGER,
             FOREIGN KEY (b_id) REFERENCES b(id)
         );",
    );
    let mut draft = db.describe("c");
    for fk in &mut draft.foreign_keys {
        if fk.columns == ["a_id"] {
            fk.on_delete = Some("SET NULL".into());
        } else {
            fk.drop = true;
        }
    }
    db.apply(&draft);
    let table = db.sql_of("c");
    assert!(!table.contains("REFERENCES b"), "{table}");
    assert!(!table.contains("REFERENCES a(id),"), "{table}");
    assert_contains(&table, &["REFERENCES \"a\" (\"id\") ON DELETE SET NULL"]);
    let again = db.describe("c");
    assert_eq!(again.foreign_keys.len(), 1);
    assert_eq!(again.foreign_keys[0].on_delete.as_deref(), Some("SET NULL"));
}

#[test]
fn foreign_keys_that_were_off_stay_off_and_only_this_table_is_checked() {
    let db = TempDb::new("fkoff");
    db.run(
        "PRAGMA foreign_keys = OFF;
         CREATE TABLE parent (id INTEGER PRIMARY KEY);
         CREATE TABLE other (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id));
         INSERT INTO other VALUES (1, 42);
         CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT);",
    );
    let mut draft = db.describe("t");
    column(&mut draft, "a").nullable = false;
    let (plan, applied) = db.apply(&draft);
    assert_eq!(db.rows("PRAGMA foreign_keys"), [["0"]]);
    assert!(!applied
        .applied
        .iter()
        .any(|sql| sql == "PRAGMA foreign_keys = ON"));
    assert_eq!(applied.applied.len() + 1, plan.statements.len());
}

#[test]
fn a_stale_draft_is_refused_inside_the_transaction() {
    let db = TempDb::new("stale");
    db.run("CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT); CREATE INDEX t_a ON t(a);");
    let mut draft = db.describe("t");
    column(&mut draft, "a").nullable = false;
    // Someone changes an index after the draft was read.
    db.run("DROP INDEX t_a; CREATE INDEX t_a ON t(a DESC);");
    let before = db.schema();
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    let err = match apply_on(db.conn(), &plan, &draft) {
        Ok(result) => panic!("applied: {result:?}"),
        Err(err) => err.to_string(),
    };
    assert!(err.contains("schema changed"), "{err}");
    assert_eq!(db.schema(), before);
}

#[test]
fn a_view_that_would_break_rolls_the_rebuild_back() {
    let db = TempDb::new("view");
    db.run(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT, b TEXT);
         CREATE VIEW v AS SELECT a FROM t;",
    );
    let before = db.schema();
    let mut draft = db.describe("t");
    column(&mut draft, "a").drop = true;
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    let err = match apply_on(db.conn(), &plan, &draft) {
        Ok(result) => panic!("applied: {result:?}"),
        Err(err) => err.to_string(),
    };
    assert!(err.contains("views stop working"), "{err}");
    assert_eq!(db.schema(), before);
}

#[test]
fn strict_without_rowid_tables_keep_their_options() {
    let db = TempDb::new("strict");
    db.run(
        "CREATE TABLE kv (k TEXT PRIMARY KEY, v ANY, n INT) WITHOUT ROWID, STRICT;
         INSERT INTO kv VALUES ('a', 1, 2);",
    );
    let mut draft = db.describe("kv");
    assert_eq!(
        draft
            .columns
            .iter()
            .map(|c| c.data_type.as_str())
            .collect::<Vec<_>>(),
        ["TEXT", "ANY", "INT"]
    );
    column(&mut draft, "n").nullable = false;
    column(&mut draft, "v").default_value = Some("0".into());
    db.apply(&draft);
    let table = db.sql_of("kv");
    assert_contains(
        &table,
        &["WITHOUT ROWID, STRICT", "v ANY DEFAULT 0", "n INT NOT NULL"],
    );
    assert_eq!(db.rows("SELECT k, v, n FROM kv"), [["a", "1", "2"]]);
}

#[test]
fn a_rename_with_a_trigger_that_uses_the_column_is_refused_with_a_rebuild() {
    let db = TempDb::new("trigger-rename");
    db.run(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT, b TEXT);
         CREATE TRIGGER t_a AFTER UPDATE ON t BEGIN SELECT NEW.a; END;",
    );
    let mut draft = db.describe("t");
    column(&mut draft, "a").name = "a2".into();
    column(&mut draft, "b").nullable = false;
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(!plan.applyable());
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("Trigger t_a uses column a")),
        "{plan:?}"
    );
    // The rename alone runs in place, and SQLite rewrites the trigger.
    let mut rename_only = db.describe("t");
    column(&mut rename_only, "a").name = "a2".into();
    let (plan, _) = db.apply(&rename_only);
    assert!(!plan.requires_rebuild);
    assert_contains(&db.sql_of("t_a"), &["a2"]);
}

#[test]
fn renames_reach_child_tables_and_the_primary_key_can_change() {
    let db = TempDb::new("pk");
    db.run(
        "CREATE TABLE p (code TEXT PRIMARY KEY, label TEXT);
         CREATE TABLE ch (id INTEGER PRIMARY KEY, pcode TEXT REFERENCES p(code));
         INSERT INTO p VALUES ('x', 'X');
         INSERT INTO ch VALUES (1, 'x');",
    );
    let mut draft = db.describe("p");
    column(&mut draft, "code").name = "ref".into();
    column(&mut draft, "label").data_type = "VARCHAR(10)".into();
    db.apply(&draft);
    assert_contains(&db.sql_of("ch"), &["REFERENCES p(\"ref\")"]);
    assert_eq!(
        db.rows("PRAGMA foreign_key_check"),
        Vec::<Vec<String>>::new()
    );

    let mut keyed = db.describe("ch");
    column(&mut keyed, "id").pk_position = 0;
    column(&mut keyed, "pcode").pk_position = 1;
    column(&mut keyed, "pcode").nullable = false;
    let (plan, _) = db.apply(&keyed);
    assert!(plan
        .statements
        .iter()
        .any(|s| s.contains("PRIMARY KEY (\"pcode\")")));
    let again = db.describe("ch");
    let pk: Vec<_> = again
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c.pk_position))
        .collect();
    assert_eq!(pk, [("id", 0), ("pcode", 1)]);
    assert_eq!(db.rows("SELECT id, pcode FROM ch"), [["1", "x"]]);
}
