//! C-06: "Don't save password, ask once per session". The password lives in
//! process memory only; the vault is never read for it.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;

use super::*;
use crate::domain::{SaveConnectionInput, VAULT_SECRET_REF};
use crate::error::AppError;
use crate::secrets::counting::CountingKeySource;
use crate::storage::sqlite::repo;

struct TempFile(PathBuf);

impl TempFile {
    fn new(tag: &str) -> Self {
        Self(std::env::temp_dir().join(format!(
            "mystique-c06-{tag}-{}-{}.sqlite",
            std::process::id(),
            uuid::Uuid::new_v4()
        )))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

fn app_on(db: AppDatabase, keys: &Arc<CountingKeySource>) -> AppServices {
    AppServices::with_secrets(
        db,
        ConnectionManager::bootstrap(),
        SecretStore::with_key_source(keys.clone()),
    )
}

fn counted() -> (AppServices, Arc<CountingKeySource>) {
    let keys = Arc::new(CountingKeySource::default());
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys);
    (app, keys)
}

fn pg(name: &str, password: Option<&str>, save_password: Option<bool>) -> SaveConnectionInput {
    SaveConnectionInput {
        id: None,
        name: name.into(),
        engine: "postgres".into(),
        color_id: "teal".into(),
        environment: "local".into(),
        read_only: false,
        notes: None,
        plugin_config: json!({"host": "db.internal", "port": 5432, "database": "app"}),
        password: password.map(Into::into),
        secret_ref: None,
        clear_secret: false,
        copy_secret_from: None,
        favorite: None,
        ssh_config: crate::domain::SshConfig::default(),
        ssh_password: None,
        clear_ssh_secret: false,
        tls_key_passphrase: None,
        query_timeout_secs: None,
        save_password,
    }
}

fn edit(id: &str, input: SaveConnectionInput) -> SaveConnectionInput {
    SaveConnectionInput {
        id: Some(id.into()),
        ..input
    }
}

fn open_req(profile: &crate::domain::ConnectionProfile) -> OpenSessionRequest {
    OpenSessionRequest {
        engine: profile.engine.clone(),
        plugin_config: profile.plugin_config.clone(),
        password: None,
        pool: None,
        session_id: None,
        profile_id: Some(profile.id.clone()),
        ssh: None,
        tls_key_passphrase: None,
        environment: None,
        connection_name: None,
        profile_read_only: false,
        use_saved_ssh: true,
    }
}

fn envelope(app: &AppServices, id: &str) -> Option<String> {
    app.db
        .with_conn(|c| repo::secrets::get(c, "connection", id, "password"))
        .unwrap()
        .map(|r| r.envelope)
}

/// Pretend an earlier connect this session typed `pw`.
fn hold(app: &AppServices, id: &str, pw: &str) {
    let pending = app.secrets.session().pending(id, pw);
    app.secrets.session().commit(pending);
}

#[test]
fn the_flag_round_trips_and_defaults_to_saving() {
    let (app, _) = counted();
    let saved = app.profiles().save(pg("on", None, None)).unwrap();
    assert!(saved.save_password, "the default saves the password");

    let off = app.profiles().save(pg("off", None, Some(false))).unwrap();
    assert!(!off.save_password);
    assert!(!app.profiles().get(&off.id).unwrap().save_password);
    let json = serde_json::to_value(app.profiles().get(&off.id).unwrap()).unwrap();
    assert_eq!(json["savePassword"], false);

    // An update that does not send the flag keeps it.
    let renamed = app
        .profiles()
        .save(edit(&off.id, pg("renamed", None, None)))
        .unwrap();
    assert!(!renamed.save_password);
    let back_on = app
        .profiles()
        .save(edit(&off.id, pg("renamed", None, Some(true))))
        .unwrap();
    assert!(back_on.save_password);
}

#[test]
fn turning_the_flag_off_deletes_the_saved_password() {
    let (app, _) = counted();
    let saved = app.profiles().save(pg("pg", Some("s3cret"), None)).unwrap();
    assert!(saved.password_set);
    assert!(envelope(&app, &saved.id).is_some());

    let off = app
        .profiles()
        .save(edit(&saved.id, pg("pg", None, Some(false))))
        .unwrap();
    assert!(!off.password_set);
    assert!(envelope(&app, &saved.id).is_none());
    let pointer = app
        .db
        .with_conn(|c| repo::connections::get_secret_ref(c, &saved.id))
        .unwrap();
    assert!(pointer.is_none());

    // A password typed while the flag is off is not stored either.
    let typed = app
        .profiles()
        .save(edit(&saved.id, pg("pg", Some("typed"), None)))
        .unwrap();
    assert!(!typed.password_set);
    assert!(envelope(&app, &saved.id).is_none());
}

#[test]
fn open_without_a_session_password_asks_and_reads_nothing() {
    let file = TempFile::new("asks");
    let keys = Arc::new(CountingKeySource::default());
    let saved = app_on(AppDatabase::open(&file.0).unwrap(), &keys)
        .profiles()
        .save(pg("pg", Some("s3cret"), None))
        .unwrap();
    // A stale envelope and pointer, as if the flag were flipped by hand.
    AppDatabase::open(&file.0)
        .unwrap()
        .with_conn(|c| {
            c.execute(
                "UPDATE connections SET save_password = 0 WHERE id = ?1",
                [saved.id.parse::<i64>().unwrap()],
            )
            .map_err(AppError::from)
        })
        .unwrap();

    // Next launch.
    let app = app_on(AppDatabase::open(&file.0).unwrap(), &keys);
    let loads = keys.loads();
    let err = app.attach_session_secrets(open_req(&saved)).unwrap_err();
    assert!(matches!(err, AppError::PasswordRequired), "{err}");
    assert!(err.to_string().starts_with("password_required"), "{err}");
    assert_eq!(keys.loads(), loads, "the vault key was loaded");
    assert_eq!(
        app.db
            .with_conn(|c| repo::connections::get_secret_ref(c, &saved.id))
            .unwrap()
            .as_deref(),
        Some(VAULT_SECRET_REF)
    );
}

#[tokio::test]
async fn a_typed_password_is_kept_and_the_next_open_reads_nothing() {
    let (app, keys) = counted();
    let file = TempFile::new("target");
    let mut input = pg("lite", None, Some(false));
    input.engine = "sqlite".into();
    input.plugin_config = json!({"path": file.0.to_string_lossy()});
    let saved = app.profiles().save(input).unwrap();

    let mut typed = open_req(&saved);
    typed.password = Some("typed-pw".into());
    typed.session_id = Some("first".into());
    app.open_session(typed).await.unwrap();

    let mut again = open_req(&saved);
    again.session_id = Some("second".into());
    let bound = app.attach_session_secrets(again.clone()).unwrap();
    assert_eq!(bound.password.as_deref(), Some("typed-pw"));
    app.open_session(again).await.unwrap();
    assert_eq!(keys.loads(), 0);
}

#[tokio::test]
async fn a_passing_test_keeps_the_typed_password_like_a_connect() {
    let (app, _) = counted();
    let file = TempFile::new("tested");
    let mut input = pg("lite", None, Some(false));
    input.engine = "sqlite".into();
    input.plugin_config = json!({"path": file.0.to_string_lossy()});
    let saved = app.profiles().save(input).unwrap();

    let err = app.test_connection(open_req(&saved)).await.unwrap_err();
    assert!(matches!(err, AppError::PasswordRequired), "{err}");

    let mut typed = open_req(&saved);
    typed.password = Some("typed-pw".into());
    assert!(app.test_connection(typed).await.unwrap().ok);
    assert_eq!(
        app.secrets.session().get(&saved.id).as_deref(),
        Some("typed-pw")
    );
}

#[test]
fn a_password_typed_for_another_host_is_not_kept() {
    let (app, _) = counted();
    let saved = app.profiles().save(pg("pg", None, Some(false))).unwrap();
    let mut req = open_req(&saved);
    req.plugin_config = json!({"host": "elsewhere.example", "port": 5432, "database": "app"});
    req.password = Some("typed".into());
    let (_, session) = bind_with_session(&app.db, &app.secrets, req, Prompt::Allowed).unwrap();
    assert!(matches!(session, SessionUse::None));
}

#[test]
fn a_refused_held_password_is_forgotten() {
    let (app, _) = counted();
    let saved = app.profiles().save(pg("pg", None, Some(false))).unwrap();
    hold(&app, &saved.id, "old");
    let (_, session) =
        bind_with_session(&app.db, &app.secrets, open_req(&saved), Prompt::Allowed).unwrap();
    let refused: Result<(), String> =
        Err("authentication failed: password authentication failed for user".into());
    settle_session_password(&app.secrets, session, &refused);
    assert!(app.secrets.session().get(&saved.id).is_none());
}

#[test]
fn delete_and_a_host_change_forget_the_session_password() {
    let (app, _) = counted();
    let saved = app.profiles().save(pg("pg", None, Some(false))).unwrap();

    // A rename keeps it.
    hold(&app, &saved.id, "pw");
    app.profiles()
        .save(edit(&saved.id, pg("renamed", None, None)))
        .unwrap();
    assert_eq!(app.secrets.session().get(&saved.id).as_deref(), Some("pw"));

    // A new host drops it.
    let mut moved = edit(&saved.id, pg("renamed", None, None));
    moved.plugin_config = json!({"host": "other.internal", "port": 5432, "database": "app"});
    app.profiles().save(moved).unwrap();
    assert!(app.secrets.session().get(&saved.id).is_none());

    // Saving the password again drops it.
    hold(&app, &saved.id, "pw");
    app.profiles()
        .save(edit(
            &saved.id,
            pg("renamed", Some("now-saved"), Some(true)),
        ))
        .unwrap();
    assert!(app.secrets.session().get(&saved.id).is_none());

    let other = app.profiles().save(pg("other", None, Some(false))).unwrap();
    hold(&app, &other.id, "pw");
    app.profiles().delete(&other.id).unwrap();
    assert!(app.secrets.session().get(&other.id).is_none());

    hold(&app, &saved.id, "pw");
    app.forget_session_passwords();
    assert!(app.secrets.session().get(&saved.id).is_none());
}

#[tokio::test]
async fn mcp_with_no_session_password_says_to_connect_in_the_app() {
    let (app, keys) = counted();
    let saved = app.profiles().save(pg("pg", None, Some(false))).unwrap();
    let err = app
        .agent_tools()
        .ensure_session_for(&saved.id, ToolCaller::Mcp)
        .await
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "This connection asks for its password each session. Connect to it in Mystique first."
    );
    assert_eq!(keys.loads(), 0);
}
