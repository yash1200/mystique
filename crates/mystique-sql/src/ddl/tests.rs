use super::diff::column_original;
use super::mysql::mysql_modify_definition;
use super::*;

fn text_column(name: &str, nullable: bool) -> ColumnDraft {
    ColumnDraft {
        name: name.into(),
        data_type: "text".into(),
        nullable,
        pk_position: 0,
        original: None,
        drop: false,
    }
}

fn existing(name: &str) -> ColumnDraft {
    let mut column = text_column(name, true);
    column.original = Some(column_original(&column));
    column
}

fn table(name: &str, columns: Vec<ColumnDraft>) -> TableDefinition {
    TableDefinition {
        database: None,
        schema: Some("public".into()),
        name: name.into(),
        kind: "table".into(),
        columns,
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
        schema_revision: None,
        validator: None,
        original_validator: None,
        create: false,
        drop_table: false,
        rename_to: None,
    }
}

#[test]
fn postgres_add_nullable_text_previews_the_alter() {
    let mut draft = table("orders", vec![existing("id"), text_column("note", true)]);
    draft.schema = Some("public".into());
    let plan = plan_table_alter(DdlTarget::POSTGRES, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert_eq!(
        plan.statements,
        vec!["ALTER TABLE \"public\".\"orders\" ADD COLUMN \"note\" text"]
    );
}

#[test]
fn not_null_without_a_default_cannot_be_applied() {
    let draft = table("orders", vec![existing("id"), text_column("note", false)]);
    let plan = plan_table_alter(DdlTarget::POSTGRES, &draft);
    assert!(!plan.applyable());
    assert!(plan.statements.is_empty());
    assert!(plan
        .warnings
        .iter()
        .any(|warning| warning.contains("nullable")));
}

#[test]
fn sqlite_add_and_renames_alter_in_place() {
    let add = table("orders", vec![existing("id"), text_column("note", true)]);
    let plan = plan_table_alter(DdlTarget::SQLITE, &add);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert!(!plan.requires_rebuild);
    assert_eq!(
        plan.statements,
        vec!["ALTER TABLE \"orders\" ADD COLUMN \"note\" text"]
    );

    let mut renamed = existing("title");
    renamed.name = "headline".into();
    let mut draft = table(
        "orders",
        vec![existing("id"), renamed, text_column("note", true)],
    );
    draft.rename_to = Some("purchases".into());
    let plan = plan_table_alter(DdlTarget::SQLITE, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert!(plan.transactional);
    assert_eq!(
        plan.statements,
        vec![
            "BEGIN",
            "ALTER TABLE \"orders\" RENAME COLUMN \"title\" TO \"headline\"",
            "ALTER TABLE \"orders\" ADD COLUMN \"note\" text",
            "ALTER TABLE \"orders\" RENAME TO \"purchases\"",
            "COMMIT",
        ]
    );
}

#[test]
fn renaming_an_indexed_column_keeps_the_index_valid() {
    let mut renamed = existing("title");
    renamed.name = "headline".into();
    let mut draft = table("orders", vec![existing("id"), renamed]);
    draft.indexes.push(IndexDraft::loaded(
        "orders_title",
        vec!["title".into()],
        false,
        false,
    ));
    let plan = plan_table_alter(DdlTarget::POSTGRES, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert_eq!(
        plan.statements,
        vec!["ALTER TABLE \"public\".\"orders\" RENAME COLUMN \"title\" TO \"headline\""]
    );
}

fn mysql_column(data_type: &str, nullable: bool, definition: Option<&str>) -> ColumnDraft {
    let mut column = ColumnDraft::loaded("c", data_type, nullable, 0);
    if let Some(original) = column.original.as_mut() {
        original.definition = definition.map(str::to_string);
    }
    column
}

fn mysql_modify(column: ColumnDraft) -> Result<String, String> {
    let original = column.original.clone().expect("original");
    mysql_modify_definition(&original, &column)
}

#[test]
fn mysql_modify_keeps_every_attribute_it_does_not_change() {
    let mut column = mysql_column(
        "varchar(20)",
        false,
        Some("varchar(20) CHARACTER SET latin1 COLLATE latin1_bin NOT NULL DEFAULT 'x y' COMMENT 'it''s (the) name'"),
    );
    column.data_type = "varchar(40)".into();
    assert_eq!(
        mysql_modify(column).as_deref(),
        Ok("varchar(40) CHARACTER SET latin1 COLLATE latin1_bin NOT NULL DEFAULT 'x y' COMMENT 'it''s (the) name'")
    );

    let mut id = mysql_column("int", false, Some("int NOT NULL AUTO_INCREMENT"));
    id.data_type = "bigint".into();
    assert_eq!(
        mysql_modify(id).as_deref(),
        Ok("bigint NOT NULL AUTO_INCREMENT")
    );

    let mut ts = mysql_column(
        "timestamp",
        true,
        Some("timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT 'touched'"),
    );
    ts.nullable = false;
    assert_eq!(
        mysql_modify(ts).as_deref(),
        Ok("timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT 'touched'")
    );
}

#[test]
fn mysql_modify_moves_null_and_drops_what_no_longer_applies() {
    // Nullable column without an explicit NULL; DEFAULT NULL goes with NOT NULL.
    let mut column = mysql_column("int", true, Some("int DEFAULT NULL COMMENT 'n'"));
    column.nullable = false;
    assert_eq!(
        mysql_modify(column).as_deref(),
        Ok("int NOT NULL COMMENT 'n'")
    );

    let mut column = mysql_column("int", false, Some("int NOT NULL DEFAULT '5'"));
    column.nullable = true;
    assert_eq!(mysql_modify(column).as_deref(), Ok("int NULL DEFAULT '5'"));

    // A charset makes no sense on a number.
    let mut column = mysql_column(
        "varchar(10)",
        true,
        Some("varchar(10) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin DEFAULT NULL"),
    );
    column.data_type = "int".into();
    assert_eq!(mysql_modify(column).as_deref(), Ok("int NULL DEFAULT NULL"));

    // Expression defaults and generated columns keep their parentheses.
    let mut column = mysql_column(
        "int",
        true,
        Some("int GENERATED ALWAYS AS ((`a` + 1)) VIRTUAL"),
    );
    column.nullable = false;
    assert_eq!(
        mysql_modify(column).as_deref(),
        Ok("int GENERATED ALWAYS AS ((`a` + 1)) VIRTUAL NOT NULL")
    );
}

#[test]
fn mysql_modify_without_a_definition_is_refused() {
    let mut column = mysql_column("int", true, None);
    column.nullable = false;
    let mut draft = table("orders", vec![column]);
    draft.schema = None;
    draft.database = Some("app".into());
    let plan = plan_table_alter(DdlTarget::MYSQL, &draft);
    assert!(!plan.applyable());
    assert!(
        plan.warnings.iter().any(|w| w.contains("Reload")),
        "{plan:?}"
    );

    // The definition must start with the type it was read with.
    let mut column = mysql_column("int", true, Some("bigint DEFAULT NULL"));
    column.nullable = false;
    assert!(mysql_modify(column).is_err());
}

#[test]
fn mysql_plan_shows_the_full_modify() {
    let mut column = mysql_column("int", false, Some("int NOT NULL AUTO_INCREMENT"));
    column.data_type = "bigint".into();
    let mut draft = table("orders", vec![column]);
    draft.schema = None;
    draft.database = Some("app".into());
    let plan = plan_table_alter(DdlTarget::MYSQL, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert_eq!(
        plan.statements,
        vec!["ALTER TABLE `app`.`orders` MODIFY COLUMN `c` bigint NOT NULL AUTO_INCREMENT"]
    );
}

#[test]
fn sqlite_refuses_every_change_that_needs_a_rebuild() {
    let mut typed = existing("note");
    typed.data_type = "integer".into();
    let mut not_null = existing("note");
    not_null.nullable = false;
    let mut dropped = existing("note");
    dropped.drop = true;
    let mut keyed = existing("note");
    keyed.pk_position = 1;
    let mut new_key = text_column("code", true);
    new_key.pk_position = 1;
    let mut with_fk = table("orders", vec![existing("id"), existing("note")]);
    with_fk.foreign_keys.push(ForeignKeyDraft {
        name: "orders_note_fk".into(),
        columns: vec!["note".into()],
        ref_schema: None,
        ref_table: "notes".into(),
        ref_columns: vec!["id".into()],
        original: None,
        drop: false,
    });
    let drafts = [
        table("orders", vec![existing("id"), typed]),
        table("orders", vec![existing("id"), not_null]),
        table("orders", vec![existing("id"), dropped]),
        table("orders", vec![existing("id"), keyed]),
        table("orders", vec![existing("id"), new_key]),
        with_fk,
    ];
    for draft in &drafts {
        let plan = plan_table_alter(DdlTarget::SQLITE, draft);
        assert!(!plan.applyable(), "{draft:?} -> {plan:?}");
        assert!(plan.statements.is_empty(), "{plan:?}");
        assert!(
            plan.warnings.iter().any(|w| w.contains("On SQLite")),
            "{plan:?}"
        );
    }
}

#[test]
fn concurrent_index_cannot_share_a_transaction() {
    let mut draft = table("orders", vec![existing("id"), text_column("note", true)]);
    draft.indexes.push(IndexDraft {
        name: "orders_note".into(),
        columns: vec!["note".into()],
        unique: false,
        primary: false,
        concurrent: true,
        original: None,
        drop: false,
    });
    let plan = plan_table_alter(DdlTarget::POSTGRES, &draft);
    assert!(!plan.applyable());
    let mut only = table("orders", vec![existing("note")]);
    only.indexes.push(IndexDraft {
        name: "orders_note".into(),
        columns: vec!["note".into()],
        unique: false,
        primary: false,
        concurrent: true,
        original: None,
        drop: false,
    });
    let plan = plan_table_alter(DdlTarget::POSTGRES, &only);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert!(!plan.transactional);
    assert!(plan.statements[0].contains("CONCURRENTLY"));
}

#[test]
fn mysql_alter_is_not_transactional() {
    let plan = plan_table_alter(
        DdlTarget::MYSQL,
        &table("orders", vec![existing("id"), text_column("note", true)]),
    );
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert!(!plan.transactional);
    assert!(plan.statements[0].contains("ADD COLUMN"));
    assert!(plan.note.as_deref().unwrap_or("").contains("MySQL"));
}

#[test]
fn mongo_columns_are_refused_and_indexes_are_commands() {
    let mut draft = table("orders", vec![text_column("note", true)]);
    draft.kind = "collection".into();
    let plan = plan_table_alter(DdlTarget::Mongo, &draft);
    assert!(!plan.applyable());
    let mut indexes = table("orders", vec![]);
    indexes.kind = "collection".into();
    indexes.indexes.push(IndexDraft {
        name: "note_1".into(),
        columns: vec!["note".into()],
        unique: false,
        primary: false,
        concurrent: false,
        original: None,
        drop: false,
    });
    let plan = plan_table_alter(DdlTarget::Mongo, &indexes);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert!(plan.statements[0].contains("createIndexes"));
    assert!(plan.statements[0].contains("note"));
}

#[test]
fn diff_does_not_guess_renames_and_orders_creates() {
    let source_orders = table("orders", vec![existing("id"), text_column("note", true)]);
    let mut source_child = table("lines", vec![existing("id"), existing("order_id")]);
    source_child.foreign_keys.push(ForeignKeyDraft {
        name: "lines_order".into(),
        columns: vec!["order_id".into(), "id".into()],
        ref_schema: None,
        ref_table: "orders".into(),
        ref_columns: vec!["id".into(), "note".into()],
        original: None,
        drop: false,
    });
    let target_orders = table("orders", vec![existing("id")]);
    let target_old = table("order_lines", vec![existing("id")]);
    let migration = plan_schema_diff(
        DdlTarget::POSTGRES,
        &[source_orders, source_child],
        &[target_orders, target_old],
        &[],
    );
    let actions: Vec<_> = migration
        .changes
        .iter()
        .map(|change| (change.name.as_str(), change.action.as_str()))
        .collect();
    assert!(
        actions.contains(&("order_lines", "drop")),
        "a different name is not a rename: {actions:?}"
    );
    assert!(actions.contains(&("lines", "create")));
    let orders = migration
        .changes
        .iter()
        .find(|change| change.name == "orders")
        .expect("orders");
    assert_eq!(orders.action, "alter");
    assert!(orders
        .plan
        .statements
        .iter()
        .any(|sql| sql.contains("ADD COLUMN")));
    let create_at = migration
        .changes
        .iter()
        .position(|change| change.name == "orders" && change.action == "create");
    let lines_at = migration
        .changes
        .iter()
        .position(|change| change.name == "lines")
        .expect("lines");
    if let Some(create_at) = create_at {
        assert!(create_at < lines_at);
    }
    let paired = plan_schema_diff(
        DdlTarget::POSTGRES,
        &[table("lines", vec![existing("id")])],
        &[table("order_lines", vec![existing("id")])],
        &[RenamePair {
            from: "lines".into(),
            to: "order_lines".into(),
        }],
    );
    assert!(paired.changes.iter().any(|change| {
        change.action == "alter"
            && change
                .plan
                .statements
                .iter()
                .any(|sql| sql.contains("RENAME TO"))
    }));
    assert!(!paired.changes.iter().any(|change| change.action == "drop"));
}

#[test]
fn an_unchanged_enum_does_not_block_adding_a_column() {
    let mut status = existing("status");
    status.data_type = "enum('open','closed')".into();
    if let Some(original) = status.original.as_mut() {
        original.data_type = status.data_type.clone();
    }
    let draft = table("orders", vec![status, text_column("note", true)]);
    let plan = plan_table_alter(DdlTarget::POSTGRES, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert!(plan.statements.iter().any(|sql| sql.contains("ADD COLUMN")));
}

#[test]
fn a_new_column_cannot_smuggle_sql_in_the_type() {
    let mut note = text_column("note", true);
    note.data_type = "text; drop table orders".into();
    let draft = table("orders", vec![existing("id"), note]);
    let plan = plan_table_alter(DdlTarget::POSTGRES, &draft);
    assert!(!plan.applyable());
    assert!(plan.statements.is_empty());
}

#[test]
fn postgres_drops_an_index_in_its_schema() {
    let mut draft = table("orders", vec![existing("note")]);
    draft.indexes.push(IndexDraft {
        name: "orders_note".into(),
        columns: vec!["note".into()],
        unique: false,
        primary: false,
        concurrent: false,
        original: Some(IndexOriginal {
            name: "orders_note".into(),
            columns: vec!["note".into()],
            unique: false,
            primary: false,
        }),
        drop: true,
    });
    let plan = plan_table_alter(DdlTarget::POSTGRES, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    assert_eq!(
        plan.statements,
        vec!["DROP INDEX \"public\".\"orders_note\""]
    );
}
