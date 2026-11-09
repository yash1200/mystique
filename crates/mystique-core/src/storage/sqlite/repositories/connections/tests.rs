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
            copy_secret_from: None,
            favorite: None,
            ssh_config: crate::domain::SshConfig::default(),
            ssh_password: None,
            clear_ssh_secret: false,
            tls_key_passphrase: None,
            query_timeout_secs: None,
            save_password: None,
        },
    )
    .unwrap();

    assert!(!saved.password_set);
    assert!(saved.password.is_none());
    assert_eq!(saved.engine, "postgres");
    assert!(saved.id.parse::<i64>().is_ok(), "id should be numeric");
    assert_eq!(saved.plugin_config["port"], 5432);

    let all = list_all(&conn).unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].password.is_none());

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
            copy_secret_from: None,
            favorite: None,
            ssh_config: crate::domain::SshConfig::default(),
            ssh_password: None,
            clear_ssh_secret: false,
            tls_key_passphrase: None,
            query_timeout_secs: None,
            save_password: None,
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
            copy_secret_from: None,
            favorite: None,
            ssh_config: crate::domain::SshConfig::default(),
            ssh_password: None,
            clear_ssh_secret: false,
            tls_key_passphrase: None,
            query_timeout_secs: None,
            save_password: None,
        },
    )
    .unwrap();

    assert_eq!(updated.name, "pg-renamed");
    assert!(updated.password.is_none());
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
            copy_secret_from: None,
            favorite: None,
            ssh_config: crate::domain::SshConfig::default(),
            ssh_password: None,
            clear_ssh_secret: false,
            tls_key_passphrase: None,
            query_timeout_secs: None,
            save_password: None,
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
            copy_secret_from: None,
            favorite: None,
            ssh_config: crate::domain::SshConfig::default(),
            ssh_password: None,
            clear_ssh_secret: false,
            tls_key_passphrase: None,
            query_timeout_secs: None,
            save_password: None,
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
            copy_secret_from: None,
            favorite: None,
            ssh_config: crate::domain::SshConfig::default(),
            ssh_password: None,
            clear_ssh_secret: false,
            tls_key_passphrase: None,
            query_timeout_secs: None,
            save_password: None,
        },
    )
    .unwrap();
    assert!(!saved.password_set);
    assert!(saved.password.is_none());
}

fn sample_input(name: &str, favorite: Option<bool>) -> SaveConnectionInput {
    SaveConnectionInput {
        id: None,
        name: name.into(),
        engine: "postgres".into(),
        color_id: "teal".into(),
        environment: "local".into(),
        read_only: true,
        notes: None,
        plugin_config: json!({}),
        password: None,
        secret_ref: None,
        clear_secret: false,
        copy_secret_from: None,
        favorite,
        ssh_config: crate::domain::SshConfig::default(),
        ssh_password: None,
        clear_ssh_secret: false,
        tls_key_passphrase: None,
        query_timeout_secs: None,
        save_password: None,
    }
}

#[test]
fn save_sets_favorite() {
    let conn = test_db();
    let saved = save(&conn, sample_input("fav", Some(true))).unwrap();
    assert!(saved.favorite);
}

#[test]
fn update_keeps_favorite_when_unset() {
    let conn = test_db();
    let saved = save(&conn, sample_input("fav", Some(true))).unwrap();
    let mut input = sample_input("fav-renamed", None);
    input.id = Some(saved.id);
    let updated = save(&conn, input).unwrap();
    assert!(updated.favorite);
    assert_eq!(updated.name, "fav-renamed");
}

#[test]
fn list_all_pins_favourites_first() {
    let conn = test_db();
    save(&conn, sample_input("zeta", None)).unwrap();
    save(&conn, sample_input("alpha", Some(true))).unwrap();
    save(&conn, sample_input("beta", None)).unwrap();

    let names: Vec<String> = list_all(&conn)
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, ["alpha", "beta", "zeta"]);
}

#[test]
fn query_timeout_round_trips_and_none_follows_the_app_setting() {
    let conn = test_db();
    let mut input = sample_input("t", None);
    input.query_timeout_secs = Some(0);
    let saved = save(&conn, input).unwrap();
    assert_eq!(saved.query_timeout_secs, Some(0), "0 (no timeout) was lost");

    let mut update = sample_input("t", None);
    update.id = Some(saved.id.clone());
    update.query_timeout_secs = Some(300);
    assert_eq!(save(&conn, update).unwrap().query_timeout_secs, Some(300));

    let mut cleared = sample_input("t", None);
    cleared.id = Some(saved.id.clone());
    assert_eq!(save(&conn, cleared).unwrap().query_timeout_secs, None);
}

#[test]
fn database_marks_round_trip_and_survive_a_profile_save() {
    use mystique_connector_api::DatabaseEnvironment;

    let conn = test_db();
    let saved = save(&conn, sample_input("dev", None)).unwrap();
    assert!(saved.database_environments.is_empty());

    let marks =
        crate::domain::with_database_environment(Vec::new(), "prod", None, Some("production"))
            .unwrap();
    let marked = set_database_environments(&conn, &saved.id, &marks).unwrap();
    assert_eq!(
        marked.database_environments,
        vec![DatabaseEnvironment {
            database: "prod".into(),
            schema: None,
            environment: "production".into(),
        }]
    );

    let mut update = sample_input("dev renamed", None);
    update.id = Some(saved.id.clone());
    let resaved = save(&conn, update).unwrap();
    assert_eq!(resaved.database_environments, marked.database_environments);

    let bad = vec![DatabaseEnvironment {
        database: "x".into(),
        schema: None,
        environment: "prod".into(),
    }];
    assert!(set_database_environments(&conn, &saved.id, &bad).is_err());
    assert!(set_database_environments(&conn, "999", &marks).is_err());
}

#[test]
fn with_database_environment_sets_replaces_and_clears_one_mark() {
    use crate::domain::with_database_environment as set;

    let marks = set(Vec::new(), "Prod", None, Some("production")).unwrap();
    let marks = set(marks, "app", Some("billing"), Some("production")).unwrap();
    let marks = set(marks, "prod", None, Some("staging")).unwrap();
    assert_eq!(marks.len(), 2);
    assert!(marks
        .iter()
        .any(|m| m.database == "prod" && m.environment == "staging"));
    let marks = set(marks, "APP", Some("billing"), None).unwrap();
    assert_eq!(marks.len(), 1);
    assert!(set(marks.clone(), " ", None, Some("local")).is_err());
    assert!(set(marks, "x", None, Some("dev")).is_err());
}
