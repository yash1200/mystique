
use super::*;
use crate::storage::sqlite::migrate;
use serde_json::json;

fn test_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrate(&conn).unwrap();
    conn
}

#[test]
fn save_list_delete_connection() {
    let conn = test_db();
    let saved = save(
        &conn,
        SaveConnectionInput {
            id: None,
            name: "local-pg".into(),
            engine: "postgres".into(),
            color_id: "teal".into(),
            environment: "local".into(),
            read_only: true,
            notes: Some("test".into()),
            plugin_config: json!({
                "host": "127.0.0.1",
                "port": 5432,
                "database": "app",
                "username": "postgres"
            }),
            password: Some("s3cret".into()),
            secret_ref: None,
            clear_secret: false,
        },
    )
    .unwrap();

    assert!(saved.password_set);
    assert_eq!(saved.password.as_deref(), Some("s3cret"));
    assert_eq!(saved.engine, "postgres");
    assert!(saved.id.parse::<i64>().is_ok(), "id should be numeric");
    assert_eq!(saved.plugin_config["port"], 5432);

    let all = list_all(&conn).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].password.as_deref(), Some("s3cret"));

    delete(&conn, &saved.id).unwrap();
    assert!(list_all(&conn).unwrap().is_empty());
}

#[test]
fn update_keeps_password_when_blank() {
    let conn = test_db();
    let saved = save(
        &conn,
        SaveConnectionInput {
            id: None,
            name: "pg".into(),
            engine: "postgres".into(),
            color_id: "teal".into(),
            environment: "local".into(),
            read_only: true,
            notes: None,
            plugin_config: json!({"host": "127.0.0.1", "database": "postgres"}),
            password: Some("keep-me".into()),
            secret_ref: None,
            clear_secret: false,
        },
    )
    .unwrap();

    let updated = save(
        &conn,
        SaveConnectionInput {
            id: Some(saved.id.clone()),
            name: "pg-renamed".into(),
            engine: "postgres".into(),
            color_id: "teal".into(),
            environment: "local".into(),
            read_only: true,
            notes: None,
            plugin_config: json!({"host": "127.0.0.1", "database": "postgres"}),
            password: None,
            secret_ref: None,
            clear_secret: false,
        },
    )
    .unwrap();

    assert_eq!(updated.name, "pg-renamed");
    assert_eq!(updated.password.as_deref(), Some("keep-me"));
    assert!(updated.password_set);
}

#[test]
fn clear_secret_removes_password() {
    let conn = test_db();
    let saved = save(
        &conn,
        SaveConnectionInput {
            id: None,
            name: "pg".into(),
            engine: "postgres".into(),
            color_id: "teal".into(),
            environment: "local".into(),
            read_only: true,
            notes: None,
            plugin_config: json!({}),
            password: Some("x".into()),
            secret_ref: None,
            clear_secret: false,
        },
    )
    .unwrap();

    let cleared = save(
        &conn,
        SaveConnectionInput {
            id: Some(saved.id),
            name: "pg".into(),
            engine: "postgres".into(),
            color_id: "teal".into(),
            environment: "local".into(),
            read_only: true,
            notes: None,
            plugin_config: json!({}),
            password: None,
            secret_ref: None,
            clear_secret: true,
        },
    )
    .unwrap();

    assert!(!cleared.password_set);
    assert!(cleared.password.is_none());
}

#[test]
fn ignores_legacy_placeholder_secret_ref() {
    let conn = test_db();
    let saved = save(
        &conn,
        SaveConnectionInput {
            id: None,
            name: "pg".into(),
            engine: "postgres".into(),
            color_id: "teal".into(),
            environment: "local".into(),
            read_only: true,
            notes: None,
            plugin_config: json!({}),
            password: None,
            secret_ref: Some("mystique-db/connection/abc".into()),
            clear_secret: false,
        },
    )
    .unwrap();
    assert!(!saved.password_set);
    assert!(saved.password.is_none());
}
