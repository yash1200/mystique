//! Structure editor MODIFY keeps the attributes the editor does not show.

mod common;

use common::{connect, run, write};
use mystique_connector_api::{plan_table_alter, ConnectorSession, DdlDialect, QueryMode};

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
    let plan = plan_table_alter(DdlDialect::Mysql, &draft);
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
