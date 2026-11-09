//! Structure editor MODIFY keeps the attributes the editor does not show.

mod common;

use common::{connect, run, write};
use mystique_connector_api::{ConnectorSession, ConstraintDraft, QueryMode, TableDefinition};
use mystique_sql::ddl::{
    plan_schema_diff_with, plan_table_alter, DdlTarget, DiffSelection, RenamePair,
};

async fn create_sql(session: &dyn ConnectorSession) -> String {
    let shown = run(session, "SHOW CREATE TABLE s4_modify", QueryMode::Write)
        .await
        .expect("show create");
    shown.rows[0][1]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| shown.rows[0][1].to_string())
}

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn modify_one_attribute_keeps_the_others() {
    let session = connect().await;
    let session = session.as_ref();
    write(session, "DROP TABLE IF EXISTS s4_modify").await;
    write(
        session,
        "CREATE TABLE s4_modify (
            id INT NOT NULL AUTO_INCREMENT PRIMARY KEY,
            ts TIMESTAMP NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT 'touched',
            name VARCHAR(20) CHARACTER SET latin1 COLLATE latin1_bin NOT NULL DEFAULT 'x' COMMENT 'the name'
        ) DEFAULT CHARSET=utf8mb4",
    )
    .await;

    let mut draft = session
        .describe_table_definition(Some("mystique"), None, "s4_modify", Some("table"))
        .await
        .expect("describe");
    for column in &mut draft.columns {
        match column.name.as_str() {
            "id" => column.data_type = "bigint".into(),
            "ts" => column.nullable = false,
            "name" => column.data_type = "varchar(40)".into(),
            _ => {}
        }
    }
    let plan = plan_table_alter(DdlTarget::MYSQL, &draft);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    session.apply_ddl(&plan, &draft).await.expect("apply");

    let create = create_sql(session).await;
    for kept in [
        "`id` bigint NOT NULL AUTO_INCREMENT",
        "`ts` timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT 'touched'",
        "`name` varchar(40) CHARACTER SET latin1 COLLATE latin1_bin NOT NULL DEFAULT 'x' COMMENT 'the name'",
    ] {
        assert!(create.contains(kept), "lost `{kept}` in:\n{create}");
    }

    // The draft is now stale: its definitions no longer match the table.
    let err = session
        .apply_ddl(&plan, &draft)
        .await
        .expect_err("stale draft must be refused");
    assert!(err.to_string().contains("Reload"), "{err}");
    assert_eq!(create_sql(session).await, create);

    write(session, "DROP TABLE s4_modify").await;
    session.close().await.ok();
}

async fn show_create(session: &dyn ConnectorSession, table: &str) -> String {
    let shown = run(
        session,
        &format!("SHOW CREATE TABLE {table}"),
        QueryMode::Write,
    )
    .await
    .expect("show create");
    shown.rows[0][1]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| shown.rows[0][1].to_string())
}

async fn describe(session: &dyn ConnectorSession, database: &str, table: &str) -> TableDefinition {
    session
        .describe_table_definition(Some(database), None, table, Some("table"))
        .await
        .expect("describe")
}

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn defaults_identity_checks_and_key_actions_round_trip() {
    let session = connect().await;
    let session = session.as_ref();
    write(session, "DROP TABLE IF EXISTS mq_eb_child").await;
    write(session, "DROP TABLE IF EXISTS mq_eb_parent").await;
    write(session, "CREATE TABLE mq_eb_parent (id INT PRIMARY KEY)").await;
    write(
        session,
        "CREATE TABLE mq_eb_child (
            id INT NOT NULL AUTO_INCREMENT PRIMARY KEY,
            qty INT NOT NULL DEFAULT 1 COMMENT 'how many',
            name VARCHAR(20) CHARACTER SET latin1 DEFAULT 'n',
            parent_id INT,
            CONSTRAINT mq_eb_child_qty CHECK (qty >= 0),
            CONSTRAINT mq_eb_child_parent FOREIGN KEY (parent_id) REFERENCES mq_eb_parent (id) ON DELETE CASCADE
        )",
    )
    .await;

    let draft = describe(session, "mystique", "mq_eb_child").await;
    assert_eq!(draft.columns[0].identity.as_deref(), Some("auto_increment"));
    assert_eq!(draft.columns[1].default_value.as_deref(), Some("'1'"));
    assert_eq!(draft.columns[2].default_value.as_deref(), Some("'n'"));
    assert!(
        draft
            .constraints
            .iter()
            .any(|c| c.kind == "check" && c.name == "mq_eb_child_qty"),
        "{:?}",
        draft.constraints
    );
    assert_eq!(draft.foreign_keys[0].on_delete.as_deref(), Some("CASCADE"));

    let mut edit = draft.clone();
    edit.columns[1].default_value = Some("'5'".into());
    edit.columns[2].default_value = None;
    for constraint in &mut edit.constraints {
        constraint.drop = true;
    }
    edit.constraints.push(ConstraintDraft {
        kind: "check".into(),
        name: "mq_eb_child_qty_small".into(),
        expression: Some("qty < 1000".into()),
        ..ConstraintDraft::default()
    });
    edit.foreign_keys[0].on_delete = Some("SET NULL".into());
    let plan = plan_table_alter(DdlTarget::MYSQL, &edit);
    assert!(plan.applyable(), "{:?}", plan.warnings);
    session.apply_ddl(&plan, &edit).await.unwrap_or_else(|err| {
        panic!("{err}\n{}", plan.statements.join(";\n"));
    });

    let create = show_create(session, "mq_eb_child").await;
    for kept in [
        "`id` int NOT NULL AUTO_INCREMENT",
        "`qty` int NOT NULL DEFAULT '5' COMMENT 'how many'",
        "`name` varchar(20) CHARACTER SET latin1",
        "ON DELETE SET NULL",
        "CONSTRAINT `mq_eb_child_qty_small` CHECK",
    ] {
        assert!(create.contains(kept), "lost `{kept}` in:\n{create}");
    }
    assert!(!create.contains("DEFAULT 'n'"), "{create}");
    assert!(!create.contains("`mq_eb_child_qty` CHECK"), "{create}");
    let again = describe(session, "mystique", "mq_eb_child").await;
    assert_eq!(again.columns[1].default_value.as_deref(), Some("'5'"));
    assert_eq!(again.columns[2].default_value, None);
    assert_eq!(again.foreign_keys[0].on_delete.as_deref(), Some("SET NULL"));

    write(session, "DROP TABLE mq_eb_child").await;
    write(session, "DROP TABLE mq_eb_parent").await;
    session.close().await.ok();
}

const DIFF_TABLES: &str = "
CREATE TABLE {db}.parent (id INT PRIMARY KEY);
CREATE TABLE {db}.orders (
    id INT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    title VARCHAR(50) NOT NULL DEFAULT 'x',
    qty INT,
    parent_id INT,
    CONSTRAINT orders_parent FOREIGN KEY (parent_id) REFERENCES {db}.parent (id) ON DELETE CASCADE
)";

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn schema_diff_pairs_renames_rewrites_keys_and_leaves_drops_off() {
    let session = connect().await;
    let session = session.as_ref();
    let (src, dst) = ("mq_eb_diff_src", "mq_eb_diff_dst");
    for db in [dst, src] {
        write(session, &format!("DROP DATABASE IF EXISTS {db}")).await;
    }
    for db in [src, dst] {
        write(session, &format!("CREATE DATABASE {db}")).await;
        for sql in DIFF_TABLES.replace("{db}", db).split(";\n") {
            write(session, sql).await;
        }
    }
    write(
        session,
        &format!("CREATE INDEX orders_qty ON {src}.orders (qty)"),
    )
    .await;
    write(
        session,
        &format!("ALTER TABLE {src}.orders ADD CONSTRAINT orders_qty_ok CHECK (qty >= 0)"),
    )
    .await;
    // The target drifted: renamed column, extra column, FK action gone.
    write(
        session,
        &format!("ALTER TABLE {dst}.orders RENAME COLUMN title TO headline"),
    )
    .await;
    write(
        session,
        &format!("ALTER TABLE {dst}.orders ADD COLUMN legacy INT"),
    )
    .await;
    write(
        session,
        &format!("ALTER TABLE {dst}.orders DROP FOREIGN KEY orders_parent"),
    )
    .await;
    write(
        session,
        &format!(
            "ALTER TABLE {dst}.orders ADD CONSTRAINT orders_parent FOREIGN KEY (parent_id) REFERENCES {dst}.parent (id)"
        ),
    )
    .await;
    write(
        session,
        &format!("INSERT INTO {dst}.orders (headline, qty) VALUES ('kept', 1)"),
    )
    .await;

    let list = |db: &'static str| async move {
        session
            .list_table_definitions(Some(db), None)
            .await
            .expect("list")
    };
    let source = list(src).await;
    let target = list(dst).await;
    // An explicit "not a rename" stops the guess.
    let apart = RenamePair {
        keep_apart: true,
        ..RenamePair::columns("orders", "title", "headline")
    };
    let refused = plan_schema_diff_with(
        DdlTarget::MYSQL,
        &source,
        &target,
        &[apart],
        &DiffSelection::default(),
    );
    let orders = refused
        .changes
        .iter()
        .find(|change| change.name == "orders")
        .expect("orders");
    assert!(
        orders.column_renames.is_empty(),
        "{:?}",
        orders.column_renames
    );

    let migration = plan_schema_diff_with(
        DdlTarget::MYSQL,
        &source,
        &target,
        &[],
        &DiffSelection::default(),
    );
    assert!(migration.warnings.is_empty(), "{:?}", migration.warnings);
    let orders = migration
        .changes
        .iter()
        .find(|change| change.name == "orders")
        .expect("orders");
    assert_eq!(
        orders.column_renames.len(),
        1,
        "{:?}",
        orders.column_renames
    );
    let sql = orders.plan.statements.join(";\n");
    assert!(sql.contains("RENAME COLUMN `headline` TO `title`"), "{sql}");
    assert!(sql.contains("DROP FOREIGN KEY `orders_parent`"), "{sql}");
    assert!(sql.contains("ON DELETE CASCADE"), "{sql}");
    assert!(sql.contains("CREATE INDEX `orders_qty`"), "{sql}");
    assert!(sql.contains("CHECK"), "{sql}");
    assert!(!sql.contains("DROP COLUMN"), "{sql}");
    for change in migration
        .changes
        .iter()
        .filter(|c| c.selected && c.action != "unchanged")
    {
        session
            .apply_ddl(&change.plan, &change.draft)
            .await
            .unwrap_or_else(|err| panic!("{err}\n{}", change.plan.statements.join(";\n")));
    }
    let after = describe(session, dst, "orders").await;
    let names: Vec<&str> = after.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["id", "title", "qty", "parent_id", "legacy"]);
    assert_eq!(after.foreign_keys[0].on_delete.as_deref(), Some("CASCADE"));
    assert!(after.indexes.iter().any(|i| i.name == "orders_qty"));
    assert!(after.constraints.iter().any(|c| c.name == "orders_qty_ok"));
    let title = run(
        session,
        &format!("SELECT title FROM {dst}.orders"),
        QueryMode::Write,
    )
    .await
    .expect("select");
    assert_eq!(title.rows[0][0], serde_json::json!("kept"));
    let parent = after.foreign_keys[0].ref_schema.as_deref();
    assert_eq!(
        parent,
        Some(dst),
        "the key points at the target's own table"
    );

    for db in [dst, src] {
        write(session, &format!("DROP DATABASE {db}")).await;
    }
    session.close().await.ok();
}
