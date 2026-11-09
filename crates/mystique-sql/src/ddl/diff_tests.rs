//! Schema diff (column renames, keys, constraints, drops) and the Postgres /
//! MySQL statements for defaults, identity, keys, and constraints.

use super::*;

fn col(name: &str, data_type: &str) -> ColumnDraft {
    ColumnDraft::loaded(name, data_type, true, 0)
}

fn pg_table(name: &str, columns: Vec<ColumnDraft>) -> TableDefinition {
    let mut table = TableDefinition::loaded(None, Some("public".into()), name, "table");
    table.columns = columns;
    table
}

fn change<'a>(migration: &'a SchemaMigration, name: &str) -> &'a SchemaChange {
    migration
        .changes
        .iter()
        .find(|change| change.name == name)
        .unwrap_or_else(|| panic!("no change {name}: {migration:?}"))
}

#[test]
fn a_renamed_column_at_the_same_place_is_guessed_as_a_rename() {
    let source = [pg_table(
        "t",
        vec![col("id", "integer"), col("title", "text")],
    )];
    let target = [pg_table(
        "t",
        vec![col("id", "integer"), col("headline", "text")],
    )];
    let migration = plan_schema_diff(DdlTarget::POSTGRES, &source, &target, &[]);
    let t = change(&migration, "t");
    assert_eq!(
        t.column_renames,
        [ColumnRename {
            from: "title".into(),
            to: "headline".into(),
            guessed: true
        }]
    );
    assert_eq!(
        t.plan.statements,
        ["ALTER TABLE \"public\".\"t\" RENAME COLUMN \"headline\" TO \"title\""]
    );
    assert!(t.drops.is_empty());
}

#[test]
fn a_different_type_or_an_ambiguous_pair_is_not_guessed() {
    let source = [pg_table(
        "t",
        vec![col("id", "integer"), col("a1", "text"), col("a2", "text")],
    )];
    let target = [pg_table(
        "t",
        vec![
            col("id", "integer"),
            col("b1", "text"),
            col("b2", "text"),
            col("n", "bigint"),
        ],
    )];
    let migration = plan_schema_diff(DdlTarget::POSTGRES, &source, &target, &[]);
    let t = change(&migration, "t");
    // a1↔b1 and a2↔b2 are at the same places, so they pair; n is a different type.
    assert_eq!(t.column_renames.len(), 2, "{:?}", t.column_renames);
    assert_eq!(
        t.drops,
        [DropItem {
            kind: "column".into(),
            name: "n".into(),
            included: false
        }]
    );
    assert!(!t
        .plan
        .statements
        .iter()
        .any(|sql| sql.contains("DROP COLUMN")));
}

#[test]
fn explicit_column_pairs_win_and_keep_apart_stops_the_guess() {
    let source = [pg_table(
        "t",
        vec![col("id", "integer"), col("title", "text")],
    )];
    let target = [pg_table(
        "t",
        vec![col("id", "integer"), col("headline", "text")],
    )];
    let apart = RenamePair {
        keep_apart: true,
        ..RenamePair::columns("t", "title", "headline")
    };
    let migration = plan_schema_diff(DdlTarget::POSTGRES, &source, &target, &[apart]);
    let t = change(&migration, "t");
    assert!(t.column_renames.is_empty());
    assert_eq!(
        t.plan.statements,
        ["ALTER TABLE \"public\".\"t\" ADD COLUMN \"title\" text"]
    );
    assert_eq!(t.drops.len(), 1);

    // An explicit pair across positions and names.
    let source = [pg_table("t", vec![col("x", "text"), col("id", "integer")])];
    let target = [pg_table(
        "t",
        vec![col("id", "integer"), col("zzz", "text")],
    )];
    let migration = plan_schema_diff(
        DdlTarget::POSTGRES,
        &source,
        &target,
        &[RenamePair::columns("t", "x", "zzz")],
    );
    let t = change(&migration, "t");
    assert_eq!(t.column_renames.len(), 1);
    assert!(!t.column_renames[0].guessed);
}

#[test]
fn drops_are_listed_but_applied_only_when_chosen() {
    let mut current = pg_table("t", vec![col("id", "integer"), col("old", "bigint")]);
    current.indexes.push(IndexDraft::loaded(
        "t_old",
        vec!["old".into()],
        false,
        false,
    ));
    current.foreign_keys.push(ForeignKeyDraft::loaded(
        "t_fk",
        vec!["id".into()],
        Some("public".into()),
        "p",
        vec!["id".into()],
    ));
    current.constraints.push(ConstraintDraft::loaded(
        "check",
        "t_pos",
        Vec::new(),
        Some("(id > 0)".into()),
    ));
    let gone = pg_table("gone", vec![col("id", "integer")]);
    let source = [pg_table("t", vec![col("id", "integer")])];
    let target = [current, gone];

    let none = plan_schema_diff(DdlTarget::POSTGRES, &source, &target, &[]);
    assert!(none.warnings.is_empty(), "{:?}", none.warnings);
    let t = change(&none, "t");
    let kinds: Vec<(&str, &str, bool)> = t
        .drops
        .iter()
        .map(|d| (d.kind.as_str(), d.name.as_str(), d.included))
        .collect();
    assert_eq!(
        kinds,
        [
            ("column", "old", false),
            ("index", "t_old", false),
            ("foreignKey", "t_fk", false),
            ("constraint", "t_pos", false)
        ]
    );
    assert!(t.plan.statements.is_empty(), "{:?}", t.plan.statements);
    let gone = change(&none, "gone");
    assert_eq!(gone.action, "drop");
    assert!(!gone.selected);

    let all = DiffSelection {
        skip: Vec::new(),
        drops: [
            ("t", "column", "old"),
            ("t", "index", "t_old"),
            ("t", "foreignKey", "t_fk"),
            ("t", "constraint", "t_pos"),
            ("gone", "table", "gone"),
        ]
        .into_iter()
        .map(|(table, kind, name)| DropKey {
            table: table.into(),
            kind: kind.into(),
            name: name.into(),
        })
        .collect(),
    };
    let every = plan_schema_diff_with(DdlTarget::POSTGRES, &source, &target, &[], &all);
    let t = change(&every, "t");
    assert_eq!(
        t.plan.statements,
        [
            "ALTER TABLE \"public\".\"t\" DROP CONSTRAINT \"t_fk\"",
            "ALTER TABLE \"public\".\"t\" DROP CONSTRAINT \"t_pos\"",
            "ALTER TABLE \"public\".\"t\" DROP COLUMN \"old\"",
            "DROP INDEX \"public\".\"t_old\"",
        ]
    );
    assert!(change(&every, "gone").selected);

    let skipped = DiffSelection {
        skip: vec!["t".into()],
        drops: Vec::new(),
    };
    let migration = plan_schema_diff_with(DdlTarget::POSTGRES, &source, &target, &[], &skipped);
    assert!(!change(&migration, "t").selected);
}

#[test]
fn a_foreign_key_changed_under_the_same_name_is_dropped_and_added() {
    let mut desired = pg_table("t", vec![col("id", "integer"), col("pid", "integer")]);
    desired.foreign_keys.push(
        ForeignKeyDraft::loaded(
            "t_p",
            vec!["pid".into()],
            Some("public".into()),
            "p",
            vec!["id".into()],
        )
        .with_actions(Some("CASCADE".into()), None),
    );
    let mut current = desired.clone();
    current.foreign_keys[0] = ForeignKeyDraft::loaded(
        "t_p",
        vec!["pid".into()],
        Some("public".into()),
        "p_old",
        vec!["id".into()],
    );
    let migration = plan_schema_diff(DdlTarget::POSTGRES, &[desired], &[current], &[]);
    assert_eq!(
        change(&migration, "t").plan.statements,
        [
            "ALTER TABLE \"public\".\"t\" DROP CONSTRAINT \"t_p\"",
            "ALTER TABLE \"public\".\"t\" ADD CONSTRAINT \"t_p\" FOREIGN KEY (\"pid\") REFERENCES \"public\".\"p\" (\"id\") ON DELETE CASCADE",
        ]
    );
}

#[test]
fn defaults_identity_checks_and_uniques_are_diffed() {
    let mut desired = pg_table(
        "t",
        vec![
            col("id", "integer").with_default(None, Some("always".into())),
            col("qty", "integer").with_default(Some("0".into()), None),
        ],
    );
    desired.constraints = vec![
        ConstraintDraft::loaded("check", "t_qty", Vec::new(), Some("(qty >= 0)".into())),
        ConstraintDraft::loaded("unique", "t_qty_key", vec!["qty".into()], None),
    ];
    let mut current = pg_table("t", vec![col("id", "integer"), col("qty", "integer")]);
    current.constraints = vec![ConstraintDraft::loaded(
        "check",
        "t_qty",
        Vec::new(),
        Some("(qty > 0)".into()),
    )];
    let migration = plan_schema_diff(DdlTarget::POSTGRES, &[desired], &[current], &[]);
    assert_eq!(
        change(&migration, "t").plan.statements,
        [
            "ALTER TABLE \"public\".\"t\" DROP CONSTRAINT \"t_qty\"",
            "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"id\" ADD GENERATED ALWAYS AS IDENTITY",
            "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"qty\" SET DEFAULT 0",
            "ALTER TABLE \"public\".\"t\" ADD CONSTRAINT \"t_qty\" CHECK ((qty >= 0))",
            "ALTER TABLE \"public\".\"t\" ADD CONSTRAINT \"t_qty_key\" UNIQUE (\"qty\")",
        ]
    );
}

#[test]
fn a_changed_partial_index_is_replayed_from_its_definition() {
    let index = |sql: &str| {
        IndexDraft::loaded("t_open", vec!["qty".into()], false, false)
            .with_definition(Some(sql.into()))
    };
    let mut desired = pg_table("t", vec![col("qty", "integer")]);
    desired.schema = Some("src".into());
    desired.indexes.push(index(
        "CREATE INDEX t_open ON src.t USING btree (qty) WHERE (qty > 0)",
    ));
    let mut current = pg_table("t", vec![col("qty", "integer")]);
    current
        .indexes
        .push(index("CREATE INDEX t_open ON public.t USING btree (qty)"));
    let migration = plan_schema_diff(DdlTarget::POSTGRES, &[desired.clone()], &[current], &[]);
    assert_eq!(
        change(&migration, "t").plan.statements,
        [
            "DROP INDEX \"public\".\"t_open\"",
            "CREATE INDEX t_open ON \"public\".\"t\" USING btree (qty) WHERE (qty > 0)",
        ]
    );
    // The same definition in another schema is no change.
    let mut same = pg_table("t", vec![col("qty", "integer")]);
    same.indexes.push(index(
        "CREATE INDEX t_open ON public.t USING btree (qty) WHERE (qty > 0)",
    ));
    let migration = plan_schema_diff(DdlTarget::POSTGRES, &[desired], &[same], &[]);
    assert_eq!(change(&migration, "t").action, "unchanged");
}

#[test]
fn postgres_identity_and_default_statements_are_ordered() {
    let mut id = ColumnDraft::loaded("id", "integer", false, 0)
        .with_default(None, Some("by default".into()));
    id.identity = None;
    id.default_value = Some("nextval('s')".into());
    let mut draft = pg_table("t", vec![id]);
    draft.schema = Some("public".into());
    let plan = plan_table_alter(DdlTarget::POSTGRES, &draft);
    assert_eq!(
        plan.statements,
        [
            "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"id\" DROP IDENTITY IF EXISTS",
            "ALTER TABLE \"public\".\"t\" ALTER COLUMN \"id\" SET DEFAULT nextval('s')",
        ]
    );
}

fn mysql_column(definition: &str, data_type: &str) -> ColumnDraft {
    let (default, identity) = mysql_column_default(definition);
    let mut column = ColumnDraft::loaded("c", data_type, true, 0).with_default(default, identity);
    if let Some(original) = column.original.as_mut() {
        original.definition = Some(definition.into());
        original.nullable = !definition.contains("NOT NULL");
    }
    column.nullable = !definition.contains("NOT NULL");
    column
}

fn mysql_plan(column: ColumnDraft) -> Vec<String> {
    let mut draft = TableDefinition::loaded(Some("app".into()), None, "t", "table");
    draft.columns.push(column);
    let plan = plan_table_alter(DdlTarget::MYSQL, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    plan.statements
}

#[test]
fn mysql_default_and_auto_increment_changes_keep_the_rest() {
    let mut column = mysql_column("int NOT NULL DEFAULT '1' COMMENT 'how many'", "int");
    assert_eq!(column.default_value.as_deref(), Some("'1'"));
    column.default_value = Some("'5'".into());
    assert_eq!(
        mysql_plan(column),
        ["ALTER TABLE `app`.`t` MODIFY COLUMN `c` int NOT NULL DEFAULT '5' COMMENT 'how many'"]
    );

    let mut column = mysql_column(
        "varchar(20) CHARACTER SET latin1 COLLATE latin1_bin DEFAULT 'n' COMMENT 'x'",
        "varchar(20)",
    );
    column.default_value = None;
    assert_eq!(
        mysql_plan(column),
        ["ALTER TABLE `app`.`t` MODIFY COLUMN `c` varchar(20) CHARACTER SET latin1 COLLATE latin1_bin NULL COMMENT 'x'"]
    );

    let mut column = mysql_column("bigint NOT NULL AUTO_INCREMENT", "bigint");
    assert_eq!(column.identity.as_deref(), Some("auto_increment"));
    column.identity = None;
    assert_eq!(
        mysql_plan(column),
        ["ALTER TABLE `app`.`t` MODIFY COLUMN `c` bigint NOT NULL"]
    );
}

#[test]
fn mysql_checks_use_drop_check() {
    let mut draft = TableDefinition::loaded(Some("app".into()), None, "t", "table");
    draft.columns.push(col("qty", "int"));
    draft.constraints.push(ConstraintDraft::loaded(
        "check",
        "t_qty",
        Vec::new(),
        Some("(`qty` > 0)".into()),
    ));
    draft.constraints[0].drop = true;
    let plan = plan_table_alter(DdlTarget::MYSQL, &draft);
    assert_eq!(
        plan.statements,
        ["ALTER TABLE `app`.`t` DROP CHECK `t_qty`"]
    );
}

#[test]
fn a_check_expression_cannot_smuggle_sql() {
    let mut draft = pg_table("t", vec![col("qty", "integer")]);
    draft.constraints.push(ConstraintDraft {
        kind: "check".into(),
        name: "bad".into(),
        expression: Some("qty > 0); DROP TABLE t; --".into()),
        ..ConstraintDraft::default()
    });
    let plan = plan_table_alter(DdlTarget::POSTGRES, &draft);
    assert!(!plan.applyable());
    assert!(plan.statements.is_empty());
}
