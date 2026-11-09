//! Live checks for per-database production (F-03) and the affected-row
//! preview (F-05) on Postgres and MySQL.
//!
//! `cargo test -p mystique-core --test live_safety -- --ignored`
//!
//! Postgres: the local `postgres` database. MySQL: `admin`/`123`, database
//! `mystique`, plus a scratch database `mq_fb_scratch` it creates and drops.
//! Every table is `mq_fb_*` and is dropped at the end.

use mystique_core::{
    ConnectionManager, ConnectorError, DatabaseEnvironment, ExecuteQueryRequest,
    OpenSessionRequest, PreviewWriteRowsRequest, QueryMode, WriteRowPreview,
};
use serde_json::{json, Value as JsonValue};
use tokio::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::const_new(());

fn open(engine: &str, session_id: &str, database: &str, environment: &str) -> OpenSessionRequest {
    let plugin_config = match engine {
        "postgres" => json!({
            "host": "127.0.0.1",
            "port": 5432,
            "database": database,
            "username": std::env::var("USER").unwrap_or_else(|_| "postgres".into()),
        }),
        _ => json!({
            "host": "127.0.0.1",
            "port": 3306,
            "database": database,
            "username": "admin",
        }),
    };
    OpenSessionRequest {
        engine: engine.into(),
        plugin_config,
        password: (engine == "mysql").then(|| "123".to_string()),
        pool: None,
        session_id: Some(session_id.into()),
        profile_id: Some(format!("{session_id}-profile")),
        ssh: None,
        tls_key_passphrase: None,
        environment: Some(environment.into()),
        connection_name: Some(session_id.into()),
        profile_read_only: false,
        use_saved_ssh: true,
    }
}

fn req(session_id: &str, text: &str) -> ExecuteQueryRequest {
    ExecuteQueryRequest {
        session_id: session_id.into(),
        text: text.into(),
        mode: QueryMode::Write,
        max_rows: None,
        timeout_ms: None,
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
    }
}

async fn write(mgr: &ConnectionManager, session_id: &str, text: &str) {
    mgr.execute_query(req(session_id, text))
        .await
        .unwrap_or_else(|err| panic!("{text}: {err}"));
}

async fn value(mgr: &ConnectionManager, session_id: &str, text: &str) -> JsonValue {
    let result = mgr
        .execute_query(req(session_id, text))
        .await
        .unwrap_or_else(|err| panic!("{text}: {err}"));
    result.rows[0][0].clone()
}

async fn preview(mgr: &ConnectionManager, session_id: &str, statement: &str) -> WriteRowPreview {
    mgr.preview_write_rows(PreviewWriteRowsRequest {
        session_id: session_id.into(),
        statement: statement.into(),
        database: None,
    })
    .await
    .unwrap_or_else(|err| panic!("{statement}: {err}"))
}

fn column(preview: &WriteRowPreview, name: &str) -> usize {
    preview
        .columns
        .iter()
        .position(|c| c.name == name)
        .unwrap_or_else(|| panic!("no column {name} in {:?}", preview.columns))
}

fn text(cell: &JsonValue) -> String {
    match cell {
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The checks both engines share. The table is `mq_fb_preview(id, name)`.
async fn previews_rows_without_writing(mgr: &ConnectionManager, sid: &str) {
    let update = preview(
        mgr,
        sid,
        "UPDATE mq_fb_preview SET name = 'new' WHERE id = 2",
    )
    .await;
    assert!(update.available, "{update:?}");
    assert_eq!(update.error, None, "{update:?}");
    assert_eq!(update.action.as_deref(), Some("update"));
    assert_eq!(update.total, Some(1));
    assert_eq!(update.rows.len(), 1);
    assert_eq!(update.new_columns, ["name (new)"]);
    let row = &update.rows[0];
    assert_eq!(text(&row[column(&update, "name")]), "two");
    assert_eq!(text(&row[column(&update, "name (new)")]), "new");

    let delete = preview(mgr, sid, "DELETE FROM mq_fb_preview WHERE id >= 2").await;
    assert_eq!(delete.action.as_deref(), Some("delete"));
    assert_eq!(delete.total, Some(2));
    assert_eq!(delete.rows.len(), 2);

    let none = preview(mgr, sid, "DELETE FROM mq_fb_preview WHERE id = 99").await;
    assert!(none.available);
    assert_eq!(none.total, Some(0));
    assert!(none.rows.is_empty());

    let insert = preview(
        mgr,
        sid,
        "INSERT INTO mq_fb_preview VALUES (10, 'ten'), (11, 'eleven')",
    )
    .await;
    assert_eq!(insert.action.as_deref(), Some("insert"));
    assert_eq!(insert.total, Some(2));
    assert_eq!(
        insert
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "name"]
    );
    assert_eq!(text(&insert.rows[1][1]), "eleven");

    let joined = preview(
        mgr,
        sid,
        "DELETE FROM mq_fb_preview WHERE id IN (1, 2); DELETE FROM mq_fb_preview",
    )
    .await;
    assert!(!joined.available);
    assert!(joined.reason.is_some());

    // Nothing above wrote: the rows are as seeded.
    assert_eq!(
        text(&value(mgr, sid, "SELECT COUNT(*) FROM mq_fb_preview").await),
        "3"
    );
    assert_eq!(
        text(&value(mgr, sid, "SELECT name FROM mq_fb_preview WHERE id = 2").await),
        "two"
    );
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn postgres_preview_and_per_database_production() {
    let _serial = SERIAL.lock().await;
    let mgr = ConnectionManager::bootstrap();
    let sid = "mq_fb_pg";
    mgr.open_session(open("postgres", sid, "postgres", "local"))
        .await
        .expect("open");
    write(
        &mgr,
        sid,
        "DROP TABLE IF EXISTS mq_fb_preview; \
         CREATE TABLE mq_fb_preview (id int PRIMARY KEY, name text); \
         INSERT INTO mq_fb_preview VALUES (1, 'one'), (2, 'two'), (3, 'three')",
    )
    .await;

    previews_rows_without_writing(&mgr, sid).await;

    // A preview whose WHERE would write is refused before it runs.
    let sneaky = preview(
        &mgr,
        sid,
        "DELETE FROM mq_fb_preview WHERE id = (SELECT nextval('mq_fb_seq'))",
    )
    .await;
    assert!(!sneaky.available || sneaky.error.is_some(), "{sneaky:?}");

    // Mark this database production: the same local connection now asks.
    mgr.set_database_marks(
        &format!("{sid}-profile"),
        vec![DatabaseEnvironment {
            database: "postgres".into(),
            schema: None,
            environment: "production".into(),
        }],
    );
    let err = mgr
        .execute_query(req(sid, "UPDATE mq_fb_preview SET name = 'x' WHERE id = 1"))
        .await
        .expect_err("marked database asks");
    let ConnectorError::WriteGuard(prompt) = err else {
        panic!("expected a prompt, got {err:?}");
    };
    assert_eq!(prompt.environment, "production");
    assert_eq!(prompt.database.as_deref(), Some("postgres"));
    // A name in another database is that database: not marked, no prompt
    // from the guard (Postgres itself refuses the cross-database name).
    let other = mgr
        .execute_query(req(
            sid,
            "UPDATE other_db.public.mq_fb_preview SET name = 'x'",
        ))
        .await
        .expect_err("postgres refuses cross-database names");
    assert!(!matches!(other, ConnectorError::WriteGuard(_)), "{other:?}");

    mgr.set_database_marks(&format!("{sid}-profile"), Vec::new());
    write(&mgr, sid, "DROP TABLE mq_fb_preview").await;
    let _ = mgr.close_session(sid).await;
}

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn mysql_preview_and_cross_database_production() {
    let _serial = SERIAL.lock().await;
    let mgr = ConnectionManager::bootstrap();
    let setup = "mq_fb_my_setup";
    mgr.open_session(open("mysql", setup, "mystique", "local"))
        .await
        .expect("open");
    write(&mgr, setup, "CREATE DATABASE IF NOT EXISTS mq_fb_scratch").await;
    write(
        &mgr,
        setup,
        "DROP TABLE IF EXISTS mystique.mq_fb_preview; \
         CREATE TABLE mystique.mq_fb_preview (id INT PRIMARY KEY, name VARCHAR(20)); \
         INSERT INTO mystique.mq_fb_preview VALUES (1, 'one'), (2, 'two'), (3, 'three'); \
         DROP TABLE IF EXISTS mq_fb_scratch.mq_fb_preview; \
         CREATE TABLE mq_fb_scratch.mq_fb_preview (id INT PRIMARY KEY, name VARCHAR(20))",
    )
    .await;

    previews_rows_without_writing(&mgr, setup).await;

    // A session on the scratch database; `mystique` is marked production.
    let sid = "mq_fb_my";
    mgr.open_session(open("mysql", sid, "mq_fb_scratch", "local"))
        .await
        .expect("open scratch");
    mgr.set_database_marks(
        &format!("{sid}-profile"),
        vec![DatabaseEnvironment {
            database: "mystique".into(),
            schema: None,
            environment: "production".into(),
        }],
    );
    write(&mgr, sid, "INSERT INTO mq_fb_preview VALUES (1, 'scratch')").await;
    write(
        &mgr,
        sid,
        "UPDATE mq_fb_scratch.mq_fb_preview SET name = 'still scratch'",
    )
    .await;
    for statement in [
        "UPDATE mystique.mq_fb_preview SET name = 'x' WHERE id = 1",
        "UPDATE `mystique`.`mq_fb_preview` SET name = 'x' WHERE id = 1",
        "USE mystique; UPDATE mq_fb_preview SET name = 'x' WHERE id = 1",
    ] {
        let err = mgr
            .execute_query(req(sid, statement))
            .await
            .expect_err(statement);
        let ConnectorError::WriteGuard(prompt) = err else {
            panic!("{statement}: expected a prompt, got {err:?}");
        };
        assert_eq!(prompt.environment, "production", "{statement}");
        assert_eq!(prompt.database.as_deref(), Some("mystique"), "{statement}");
    }
    assert_eq!(
        text(
            &value(
                &mgr,
                setup,
                "SELECT name FROM mystique.mq_fb_preview WHERE id = 1"
            )
            .await
        ),
        "one",
        "the prompts stopped every write to the production database"
    );

    let _ = mgr.close_session(sid).await;
    write(
        &mgr,
        setup,
        "DROP TABLE mystique.mq_fb_preview; DROP DATABASE mq_fb_scratch",
    )
    .await;
    let _ = mgr.close_session(setup).await;
}
