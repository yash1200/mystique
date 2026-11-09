//! SQLite rebuild plans: the exact statements, built from the original text.

use super::*;

fn loaded(name: &str, data_type: &str, nullable: bool, pk: u32) -> ColumnDraft {
    ColumnDraft::loaded(name, data_type, nullable, pk)
}

fn sqlite_table(sql: &str, columns: Vec<ColumnDraft>) -> TableDefinition {
    let mut draft = TableDefinition::loaded(None, None, "t", "table");
    draft.schema_revision = Some(sql.into());
    draft.columns = columns;
    draft
}

fn column<'a>(draft: &'a mut TableDefinition, name: &str) -> &'a mut ColumnDraft {
    draft
        .columns
        .iter_mut()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("{name}"))
}

#[test]
fn a_type_change_rebuilds_from_the_original_text() {
    let sql = "CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, a TEXT COLLATE NOCASE DEFAULT 'x' CHECK (a <> ''), b) STRICT";
    let mut draft = sqlite_table(
        sql,
        vec![
            loaded("id", "INTEGER", true, 1).with_default(None, Some("autoincrement".into())),
            loaded("a", "TEXT", true, 0).with_default(Some("'x'".into()), None),
            loaded("b", "", true, 0),
        ],
    );
    draft.indexes.push(
        IndexDraft::loaded("t_a", vec!["a".into()], false, false)
            .with_definition(Some("CREATE INDEX t_a ON t(a) WHERE a > 'm'".into())),
    );
    draft.triggers.push(TriggerDefinition {
        name: "t_trg".into(),
        sql: "CREATE TRIGGER t_trg AFTER INSERT ON t BEGIN SELECT 1; END".into(),
    });
    column(&mut draft, "a").data_type = "VARCHAR(10)".into();
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert!(plan.requires_rebuild && plan.transactional);
    assert_eq!(
        plan.statements,
        [
            "PRAGMA foreign_keys = OFF",
            "BEGIN IMMEDIATE",
            "CREATE TABLE \"mystique_rebuild_t\" (id INTEGER PRIMARY KEY AUTOINCREMENT, a VARCHAR(10) COLLATE NOCASE DEFAULT 'x' CHECK (a <> ''), b) STRICT",
            "INSERT INTO \"mystique_rebuild_t\" (\"id\", \"a\", \"b\") SELECT \"id\", \"a\", \"b\" FROM \"t\"",
            "DELETE FROM sqlite_sequence WHERE name = 'mystique_rebuild_t'",
            "INSERT INTO sqlite_sequence (name, seq) SELECT 'mystique_rebuild_t', seq FROM sqlite_sequence WHERE name = 't'",
            "DROP TABLE \"t\"",
            "PRAGMA legacy_alter_table = ON",
            "ALTER TABLE \"mystique_rebuild_t\" RENAME TO \"t\"",
            "PRAGMA legacy_alter_table = OFF",
            "CREATE INDEX t_a ON t(a) WHERE a > 'm'",
            "CREATE TRIGGER t_trg AFTER INSERT ON t BEGIN SELECT 1; END",
            "PRAGMA foreign_key_check",
            "COMMIT",
            "PRAGMA foreign_keys = ON",
        ]
    );
    assert!(plan
        .note
        .as_deref()
        .is_some_and(|note| note.contains("1 index(es) and 1 trigger(s)")));
}

#[test]
fn a_table_without_an_integer_key_keeps_its_rowids() {
    let sql = "CREATE TABLE t (a TEXT, b INT)";
    let mut draft = sqlite_table(
        sql,
        vec![loaded("a", "TEXT", true, 0), loaded("b", "INT", true, 0)],
    );
    column(&mut draft, "b").nullable = false;
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert!(plan.statements.contains(
        &"INSERT INTO \"mystique_rebuild_t\" (rowid, \"a\", \"b\") SELECT rowid, \"a\", \"b\" FROM \"t\""
            .to_string()
    ));
    assert!(plan
        .statements
        .contains(&"CREATE TABLE \"mystique_rebuild_t\" (a TEXT, b INT NOT NULL)".to_string()));
}

#[test]
fn renames_run_in_place_first_and_flow_into_the_new_text() {
    let sql = "CREATE TABLE t (\n  id INTEGER PRIMARY KEY,\n  a TEXT,\n  b TEXT CHECK (length(a) < 5),\n  UNIQUE (a, b)\n)";
    let mut draft = sqlite_table(
        sql,
        vec![
            loaded("id", "INTEGER", true, 1),
            loaded("a", "TEXT", true, 0),
            loaded("b", "TEXT", true, 0),
        ],
    );
    draft.indexes.push(
        IndexDraft::loaded("t_ab", vec!["a".into(), "b".into()], false, false).with_definition(
            Some("CREATE INDEX t_ab ON t (lower(a), b COLLATE nocase)".into()),
        ),
    );
    column(&mut draft, "a").name = "alpha".into();
    column(&mut draft, "b").nullable = false;
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert_eq!(
        plan.statements[2],
        "ALTER TABLE \"t\" RENAME COLUMN \"a\" TO \"alpha\""
    );
    assert_eq!(
        plan.statements[3],
        "CREATE TABLE \"mystique_rebuild_t\" (\n  id INTEGER PRIMARY KEY,\n  \"alpha\" TEXT,\n  b TEXT CHECK (length(\"alpha\") < 5) NOT NULL,\n  UNIQUE (\"alpha\", b)\n)"
    );
    assert!(plan
        .statements
        .contains(&"CREATE INDEX t_ab ON t (lower(\"alpha\"), b COLLATE nocase)".to_string()));
}

#[test]
fn a_dropped_column_takes_its_constraints_indexes_and_triggers_along() {
    let sql = "CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT CHECK (a <> ''), b TEXT, CHECK (b > a), UNIQUE (a))";
    let mut draft = sqlite_table(
        sql,
        vec![
            loaded("id", "INTEGER", true, 1),
            loaded("a", "TEXT", true, 0),
            loaded("b", "TEXT", true, 0),
        ],
    );
    draft.indexes.push(
        IndexDraft::loaded("t_a", vec!["a".into()], false, false)
            .with_definition(Some("CREATE INDEX t_a ON t(a)".into())),
    );
    draft.triggers.push(TriggerDefinition {
        name: "t_a_trg".into(),
        sql: "CREATE TRIGGER t_a_trg AFTER UPDATE OF a ON t BEGIN SELECT 1; END".into(),
    });
    column(&mut draft, "a").drop = true;
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert_eq!(
        plan.statements[2],
        "CREATE TABLE \"mystique_rebuild_t\" (id INTEGER PRIMARY KEY, b TEXT)"
    );
    assert!(!plan.statements.iter().any(|sql| sql.contains("t_a")));
    let note = plan.note.unwrap_or_default();
    for said in [
        "CHECK (b > a) uses dropped column a",
        "UNIQUE (a) uses dropped column a",
        "Index t_a uses dropped column a",
        "Trigger t_a_trg uses dropped column a",
    ] {
        assert!(note.contains(said), "{said}: {note}");
    }
}

#[test]
fn keys_and_constraints_are_added_changed_and_dropped_in_the_text() {
    let sql = "CREATE TABLE t (id INTEGER PRIMARY KEY, pid INTEGER REFERENCES p(id), qid INTEGER, CONSTRAINT q_fk FOREIGN KEY (qid) REFERENCES q (id), CONSTRAINT pos CHECK (pid > 0))";
    let mut draft = sqlite_table(
        sql,
        vec![
            loaded("id", "INTEGER", true, 1),
            loaded("pid", "INTEGER", true, 0),
            loaded("qid", "INTEGER", true, 0),
        ],
    );
    draft.foreign_keys = vec![
        ForeignKeyDraft::loaded("fk_t_1", vec!["pid".into()], None, "p", vec!["id".into()]),
        ForeignKeyDraft::loaded("q_fk", vec!["qid".into()], None, "q", vec!["id".into()]),
    ];
    draft.constraints = vec![ConstraintDraft::loaded(
        "check",
        "pos",
        Vec::new(),
        Some("pid > 0".into()),
    )];
    // Change one key's action, drop the other, swap the CHECK, add a UNIQUE.
    draft.foreign_keys[0].on_delete = Some("CASCADE".into());
    draft.foreign_keys[1].drop = true;
    draft.constraints[0].expression = Some("pid >= 0".into());
    draft.constraints.push(ConstraintDraft {
        kind: "unique".into(),
        name: "u_pid".into(),
        columns: vec!["pid".into()],
        ..ConstraintDraft::default()
    });
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert_eq!(
        plan.statements[2],
        "CREATE TABLE \"mystique_rebuild_t\" (id INTEGER PRIMARY KEY, pid INTEGER, qid INTEGER, CONSTRAINT \"pos\" CHECK (pid >= 0), CONSTRAINT \"u_pid\" UNIQUE (\"pid\"), CONSTRAINT \"fk_t_1\" FOREIGN KEY (\"pid\") REFERENCES \"p\" (\"id\") ON DELETE CASCADE)"
    );
}

#[test]
fn the_primary_key_and_autoincrement_can_change() {
    let sql = "CREATE TABLE t (id INTEGER, code TEXT, PRIMARY KEY (code))";
    let mut draft = sqlite_table(
        sql,
        vec![
            loaded("id", "INTEGER", true, 0),
            loaded("code", "TEXT", true, 1),
        ],
    );
    column(&mut draft, "code").pk_position = 0;
    column(&mut draft, "id").pk_position = 1;
    column(&mut draft, "id").identity = Some("autoincrement".into());
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert_eq!(
        plan.statements[2],
        "CREATE TABLE \"mystique_rebuild_t\" (id INTEGER PRIMARY KEY AUTOINCREMENT, code TEXT)"
    );

    // AUTOINCREMENT on a composite key is refused.
    column(&mut draft, "code").pk_position = 2;
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(plan
        .warnings
        .iter()
        .any(|w| w.contains("AUTOINCREMENT needs the primary key to be one INTEGER column")));
}

#[test]
fn unreadable_or_stale_text_is_refused() {
    let mut virtual_table = sqlite_table(
        "CREATE VIRTUAL TABLE t USING fts5(a)",
        vec![loaded("a", "", true, 0)],
    );
    column(&mut virtual_table, "a").nullable = false;
    let plan = plan_table_alter(DdlTarget::SQLITE, &virtual_table);
    assert!(
        plan.warnings.iter().any(|w| w.contains("virtual table")),
        "{plan:?}"
    );

    let mut stale = sqlite_table(
        "CREATE TABLE t (a TEXT)",
        vec![loaded("b", "TEXT", true, 0)],
    );
    column(&mut stale, "b").nullable = false;
    let plan = plan_table_alter(DdlTarget::SQLITE, &stale);
    assert!(
        plan.warnings.iter().any(|w| w.contains("Reload")),
        "{plan:?}"
    );
}

#[test]
fn a_rename_with_a_trigger_that_names_the_column_is_refused_in_a_rebuild() {
    let mut draft = sqlite_table(
        "CREATE TABLE t (a TEXT, b TEXT)",
        vec![loaded("a", "TEXT", true, 0), loaded("b", "TEXT", true, 0)],
    );
    draft.triggers.push(TriggerDefinition {
        name: "t_trg".into(),
        sql: "CREATE TRIGGER t_trg AFTER UPDATE ON t BEGIN UPDATE other SET a = NEW.a; END".into(),
    });
    column(&mut draft, "a").name = "a2".into();
    column(&mut draft, "b").data_type = "INT".into();
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(plan
        .warnings
        .iter()
        .any(|w| w.contains("Trigger t_trg uses column a")));
}

#[test]
fn add_column_with_a_default_and_untyped_columns_stay_in_place() {
    let mut draft = sqlite_table("CREATE TABLE t (a)", vec![loaded("a", "", true, 0)]);
    draft.columns.push(ColumnDraft {
        name: "n".into(),
        data_type: String::new(),
        nullable: false,
        default_value: Some("0".into()),
        ..ColumnDraft::default()
    });
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert!(!plan.requires_rebuild);
    assert_eq!(
        plan.statements,
        ["ALTER TABLE \"t\" ADD COLUMN \"n\" NOT NULL DEFAULT 0"]
    );
}

#[test]
fn a_default_cannot_smuggle_a_second_statement() {
    for bad in ["0; DROP TABLE t", "0 -- x", "(1", "'a"] {
        let mut draft = sqlite_table("CREATE TABLE t (a)", vec![loaded("a", "", true, 0)]);
        column(&mut draft, "a").default_value = Some(bad.into());
        let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
        assert!(!plan.applyable(), "{bad}");
        assert!(plan.statements.is_empty());
    }
}

#[test]
fn creating_a_sqlite_table_writes_keys_inline() {
    let mut draft = TableDefinition::loaded(None, None, "t", "table");
    draft.create = true;
    draft.columns = vec![
        ColumnDraft {
            name: "id".into(),
            data_type: "INTEGER".into(),
            nullable: false,
            pk_position: 1,
            identity: Some("autoincrement".into()),
            ..ColumnDraft::default()
        },
        ColumnDraft {
            name: "pid".into(),
            data_type: "INTEGER".into(),
            nullable: true,
            default_value: Some("1".into()),
            ..ColumnDraft::default()
        },
    ];
    draft.foreign_keys.push(ForeignKeyDraft {
        name: "t_p".into(),
        columns: vec!["pid".into()],
        ref_table: "p".into(),
        ref_columns: vec!["id".into()],
        on_delete: Some("CASCADE".into()),
        ..ForeignKeyDraft::default()
    });
    draft.constraints.push(ConstraintDraft {
        kind: "check".into(),
        expression: Some("pid > 0".into()),
        ..ConstraintDraft::default()
    });
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert_eq!(
        plan.statements,
        ["CREATE TABLE \"t\" (\"id\" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, \"pid\" INTEGER DEFAULT 1, CHECK (pid > 0), CONSTRAINT \"t_p\" FOREIGN KEY (\"pid\") REFERENCES \"p\" (\"id\") ON DELETE CASCADE)"]
    );
}
