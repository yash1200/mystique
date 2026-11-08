//! Keychain access counts (K-01…K-04) under the vault, and host binding
//! (R-06, R-07, R-16). A counting key source stands in for the one vault
//! item; a counting backend stands in for items older builds left behind.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::domain::{
    scoped_secret_account, scoped_ssh_secret_account, scoped_tls_secret_account, secret_account,
    user_secret_account, SaveConnectionInput, SshAuth, SshConfig, SshTunnelRequest,
    VAULT_SECRET_REF,
};
use crate::secrets::counting::{CountingBackend, CountingKeySource};
use crate::secrets::KeySource;
use crate::storage::sqlite::repo;

struct TempDb(PathBuf);

impl TempDb {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "mystique-{tag}-{}-{}.sqlite",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        Self(path)
    }

    fn open(&self) -> AppDatabase {
        AppDatabase::open(&self.0).unwrap()
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

/// One process: a fresh vault over a shared key source and old items.
fn app_on(
    db: AppDatabase,
    keys: &Arc<CountingKeySource>,
    items: &Arc<CountingBackend>,
) -> AppServices {
    AppServices::with_secrets(
        db,
        ConnectionManager::bootstrap(),
        SecretStore::from_parts(keys.clone(), Some(items.clone())),
    )
}

fn fakes() -> (Arc<CountingKeySource>, Arc<CountingBackend>) {
    (
        Arc::new(CountingKeySource::default()),
        Arc::new(CountingBackend::default()),
    )
}

fn pg_input(name: &str) -> SaveConnectionInput {
    SaveConnectionInput {
        id: None,
        name: name.into(),
        engine: "postgres".into(),
        color_id: "teal".into(),
        environment: "local".into(),
        read_only: false,
        notes: None,
        plugin_config: json!({"host": "db.internal", "port": 5432, "database": "app"}),
        password: Some("pg-pw".into()),
        secret_ref: None,
        clear_secret: false,
        copy_secret_from: None,
        favorite: None,
        ssh_config: SshConfig::default(),
        ssh_password: None,
        clear_ssh_secret: false,
        tls_key_passphrase: None,
        query_timeout_secs: None,
        save_password: None,
    }
}

fn bastion() -> SshConfig {
    SshConfig {
        enabled: true,
        host: "bastion.example".into(),
        port: 22,
        user: "ubuntu".into(),
        auth: SshAuth::Password,
        private_key_path: String::new(),
    }
}

/// Password, bastion password, and an encrypted TLS client key.
fn full_input() -> SaveConnectionInput {
    let mut input = pg_input("full");
    input.plugin_config = json!({
        "host": "db.internal",
        "port": 5432,
        "database": "app",
        "sslMode": "verify-full",
        "clientCertPath": "/certs/client.crt",
        "clientKeyPath": "/certs/client.key"
    });
    input.ssh_config = bastion();
    input.ssh_password = Some("ssh-pw".into());
    input.tls_key_passphrase = Some("tls-pp".into());
    input
}

fn full_pg(app: &AppServices) -> crate::domain::ConnectionProfile {
    app.profiles().save(full_input()).unwrap()
}

fn mongo_input() -> SaveConnectionInput {
    let mut input = pg_input("mongo");
    input.engine = "mongodb".into();
    input.password = None;
    input.plugin_config = json!({
        "uri": "mongodb://admin:s%40cret@127.0.0.1:27017/app?authSource=admin"
    });
    input
}

fn open_req(profile: &crate::domain::ConnectionProfile) -> OpenSessionRequest {
    OpenSessionRequest {
        engine: profile.engine.clone(),
        plugin_config: profile.plugin_config.clone(),
        password: None,
        pool: None,
        session_id: Some(profile.id.clone()),
        profile_id: Some(profile.id.clone()),
        ssh: None,
        tls_key_passphrase: None,
        environment: None,
        connection_name: None,
        profile_read_only: false,
        use_saved_ssh: true,
    }
}

fn install_of(app: &AppServices) -> String {
    app.db.with_conn(crate::secrets::install_id).unwrap()
}

fn refs(app: &AppServices, id: &str) -> (Option<String>, Option<String>) {
    app.db
        .with_conn(|c| {
            Ok((
                repo::connections::get_secret_ref(c, id)?,
                repo::connections::get_ssh_secret_ref(c, id)?,
            ))
        })
        .unwrap()
}

fn has_envelope(app: &AppServices, id: &str, kind: &str) -> bool {
    app.db
        .with_conn(|c| repo::secrets::exists(c, "connection", id, kind))
        .unwrap()
}

/// A Phase A row: secrets in `conn.<install>.<id>` items, pointers to them.
fn phase_a_row(
    app: &AppServices,
    items: &CountingBackend,
    input: SaveConnectionInput,
) -> crate::domain::ConnectionProfile {
    let mut bare = input.clone();
    bare.password = None;
    bare.ssh_password = None;
    bare.tls_key_passphrase = None;
    let saved = app.profiles().save(bare).unwrap();
    let install = install_of(app);
    let id = saved.id.as_str();
    let mut cfg = saved.plugin_config.clone();
    if let Some(pw) = input.password.as_deref() {
        let account = scoped_secret_account(&install, id);
        items.plant(&account, pw);
        app.db
            .with_conn(|c| repo::connections::set_secret_ref(c, id, Some(&account)))
            .unwrap();
    }
    if let Some(secret) = input.ssh_password.as_deref() {
        let account = scoped_ssh_secret_account(&install, id);
        items.plant(&account, secret);
        app.db
            .with_conn(|c| repo::connections::set_ssh_secret_ref(c, id, Some(&account)))
            .unwrap();
    }
    if let Some(pp) = input.tls_key_passphrase.as_deref() {
        items.plant(&scoped_tls_secret_account(&install, id), pp);
        cfg["clientKeyPassphraseSaved"] = json!(true);
    }
    app.db
        .with_conn(|c| repo::connections::set_plugin_config(c, id, &cfg))
        .unwrap();
    app.profiles().get(id).unwrap()
}

#[test]
fn startup_reads_no_keychain_item_and_loads_no_key() {
    let file = TempDb::new("boot");
    let (keys, items) = fakes();
    let (plaintext_id, placeholder_id, legacy_id) = {
        let app = app_on(file.open(), &keys, &items);
        full_pg(&app);
        app.profiles().save(pg_input("pw-only")).unwrap();
        app.profiles().save(mongo_input()).unwrap();
        // Older rows: a `conn.<id>` pointer, plaintext, and a placeholder.
        let legacy = app.profiles().save(pg_input("legacy")).unwrap();
        items.plant(&secret_account(&legacy.id), "legacy-pw");
        app.db
            .with_conn(|c| {
                repo::connections::set_secret_ref(c, &legacy.id, Some(&secret_account(&legacy.id)))
            })
            .unwrap();
        let plain = app.profiles().save(pg_input("plain")).unwrap();
        app.db
            .with_conn(|c| repo::connections::set_secret_ref(c, &plain.id, Some("plain-pw")))
            .unwrap();
        let placeholder = app.profiles().save(pg_input("placeholder")).unwrap();
        app.db
            .with_conn(|c| {
                repo::connections::set_secret_ref(c, &placeholder.id, Some("mystique/conn/9"))
            })
            .unwrap();
        (plain.id, placeholder.id, legacy.id)
    };
    keys.reset();
    items.reset_reads();

    // A cold launch: construct the services, then run the startup cleanup.
    let app = app_on(file.open(), &keys, &items);
    app.migrate_legacy_secrets().unwrap();
    app.profiles().list().unwrap();
    assert_eq!(keys.loads(), 0, "startup loaded the vault key");
    assert_eq!(
        items.total_reads(),
        0,
        "startup read {:?}",
        items.read_accounts()
    );
    assert_eq!(app.secrets.status().state, "idle");
    // Placeholders are cleared; plaintext and old pointers wait for first use.
    assert!(refs(&app, &placeholder_id).0.is_none());
    assert_eq!(refs(&app, &plaintext_id).0.as_deref(), Some("plain-pw"));
    assert_eq!(
        refs(&app, &legacy_id).0.as_deref(),
        Some(secret_account(&legacy_id).as_str())
    );

    // The flag makes it one-time.
    app.db
        .with_conn(|c| {
            repo::connections::set_secret_ref(c, &placeholder_id, Some("mystique/conn/9"))
        })
        .unwrap();
    app.migrate_legacy_secrets().unwrap();
    assert_eq!(
        refs(&app, &placeholder_id).0.as_deref(),
        Some("mystique/conn/9")
    );
    assert_eq!(keys.loads() + items.total_reads(), 0);
}

#[test]
fn connect_loads_the_vault_key_once_even_in_parallel() {
    let file = TempDb::new("connect");
    let (keys, items) = fakes();
    let (profile, other) = {
        let app = app_on(file.open(), &keys, &items);
        (
            full_pg(&app),
            app.profiles().save(pg_input("other")).unwrap(),
        )
    };
    // A new process, whose key source is slow so first uses overlap.
    let slow = Arc::new(CountingKeySource::slow(Duration::from_millis(40)));
    let key = keys.load(crate::secrets::Prompt::Allowed).unwrap();
    slow.seed(key);
    let app = Arc::new(app_on(file.open(), &slow, &items));
    items.reset_reads();
    let handles: Vec<_> = (0..6)
        .map(|i| {
            let app = Arc::clone(&app);
            let req = open_req(if i % 2 == 0 { &profile } else { &other });
            std::thread::spawn(move || app.attach_session_secrets(req).unwrap())
        })
        .collect();
    for (i, handle) in handles.into_iter().enumerate() {
        let filled = handle.join().unwrap();
        assert_eq!(filled.password.as_deref(), Some("pg-pw"));
        if i % 2 == 0 {
            assert_eq!(filled.tls_key_passphrase.as_deref(), Some("tls-pp"));
            let ssh = filled.ssh.expect("saved tunnel");
            assert_eq!(ssh.password.as_deref(), Some("ssh-pw"));
        }
    }
    assert_eq!(slow.loads(), 1, "one vault item, read once");
    assert_eq!(items.total_reads(), 0, "{:?}", items.read_accounts());

    // Later connects in the same process read nothing.
    app.attach_session_secrets(open_req(&profile)).unwrap();
    assert_eq!(slow.loads(), 1);
}

#[test]
fn a_profile_without_saved_secrets_loads_no_key() {
    let (keys, items) = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys, &items);
    let mut plain = pg_input("no-pw");
    plain.password = None;
    let plain = app.profiles().save(plain).unwrap();
    let mut keyed = pg_input("key-no-pass");
    keyed.password = None;
    keyed.plugin_config["clientKeyPath"] = json!("/certs/client.key");
    let keyed = app.profiles().save(keyed).unwrap();
    assert_eq!(
        keyed.plugin_config["clientKeyPassphraseSaved"],
        json!(false)
    );
    for profile in [&plain, &keyed] {
        let filled = app.attach_session_secrets(open_req(profile)).unwrap();
        assert!(filled.password.is_none());
        assert!(filled.tls_key_passphrase.is_none());
    }
    assert_eq!(keys.loads(), 0);
    assert_eq!(items.total_reads(), 0);

    // An update that leaves the passphrase blank keeps the saved one.
    let full = full_pg(&app);
    let mut update = pg_input("full");
    update.id = Some(full.id.clone());
    update.password = None;
    update.plugin_config = full.plugin_config.clone();
    update.ssh_config = full.ssh.clone();
    let updated = app.profiles().save(update).unwrap();
    assert_eq!(
        updated.plugin_config["clientKeyPassphraseSaved"],
        json!(true)
    );
    let filled = app.attach_session_secrets(open_req(&updated)).unwrap();
    assert_eq!(filled.tls_key_passphrase.as_deref(), Some("tls-pp"));
}

#[test]
fn an_old_keychain_row_is_read_once_moved_and_deleted() {
    let file = TempDb::new("migrate");
    let (keys, items) = fakes();
    let (profile, install) = {
        let app = app_on(file.open(), &keys, &items);
        (phase_a_row(&app, &items, full_input()), install_of(&app))
    };
    let accounts = [
        scoped_secret_account(&install, &profile.id),
        scoped_ssh_secret_account(&install, &profile.id),
        scoped_tls_secret_account(&install, &profile.id),
    ];
    keys.reset();
    items.reset_reads();

    // First connect after the upgrade.
    let app = app_on(file.open(), &keys, &items);
    let filled = app.attach_session_secrets(open_req(&profile)).unwrap();
    assert_eq!(filled.password.as_deref(), Some("pg-pw"));
    assert_eq!(filled.tls_key_passphrase.as_deref(), Some("tls-pp"));
    assert_eq!(
        filled.ssh.and_then(|s| s.password).as_deref(),
        Some("ssh-pw")
    );
    for account in &accounts {
        assert_eq!(items.reads_of(account), 1, "{account}");
        assert!(!items.contains(account), "{account} deleted after the move");
    }
    assert_eq!(items.total_reads(), 3, "{:?}", items.read_accounts());
    assert_eq!(keys.loads(), 1);
    let (password_ref, ssh_ref) = refs(&app, &profile.id);
    assert_eq!(password_ref.as_deref(), Some(VAULT_SECRET_REF));
    assert_eq!(ssh_ref.as_deref(), Some(VAULT_SECRET_REF));
    for kind in ["password", "ssh", "tls"] {
        assert!(has_envelope(&app, &profile.id, kind), "{kind}");
    }

    // Second connect, in a new process: no keychain item, the vault key once.
    keys.reset();
    items.reset_reads();
    let app = app_on(file.open(), &keys, &items);
    let again = app.attach_session_secrets(open_req(&profile)).unwrap();
    assert_eq!(again.password.as_deref(), Some("pg-pw"));
    assert_eq!(items.total_reads(), 0, "{:?}", items.read_accounts());
    assert_eq!(keys.loads(), 1);
}

#[test]
fn the_oldest_pointers_move_too() {
    let (keys, items) = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys, &items);
    let mut bare = pg_input("aug");
    bare.password = None;
    let profile = app.profiles().save(bare).unwrap();
    let pointer = format!("connection.{}.password", uuid::Uuid::new_v4());
    items.plant(&pointer, "aug-pw");
    app.db
        .with_conn(|c| repo::connections::set_secret_ref(c, &profile.id, Some(&pointer)))
        .unwrap();
    let filled = app.attach_session_secrets(open_req(&profile)).unwrap();
    assert_eq!(filled.password.as_deref(), Some("aug-pw"));
    assert!(!items.contains(&pointer));
    assert_eq!(refs(&app, &profile.id).0.as_deref(), Some(VAULT_SECRET_REF));
}

#[test]
fn a_missing_old_item_keeps_the_pointer_and_is_not_reread() {
    let (keys, items) = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys, &items);
    let mut bare = pg_input("gone");
    bare.password = None;
    let profile = app.profiles().save(bare).unwrap();
    let pointer = secret_account(&profile.id);
    app.db
        .with_conn(|c| repo::connections::set_secret_ref(c, &profile.id, Some(&pointer)))
        .unwrap();
    for _ in 0..3 {
        let err = app.attach_session_secrets(open_req(&profile)).unwrap_err();
        assert!(err.to_string().contains("missing"), "{err}");
    }
    assert_eq!(items.reads_of(&pointer), 1);
    assert_eq!(keys.loads(), 0);
    assert_eq!(refs(&app, &profile.id).0.as_deref(), Some(pointer.as_str()));
}

#[test]
fn plaintext_and_placeholder_rows_behave_as_before() {
    let (keys, items) = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys, &items);
    let mut bare = pg_input("plain");
    bare.password = None;
    let plain = app.profiles().save(bare.clone()).unwrap();
    app.db
        .with_conn(|c| repo::connections::set_secret_ref(c, &plain.id, Some("plain-pw")))
        .unwrap();
    let placeholder = app.profiles().save(bare).unwrap();
    app.db
        .with_conn(|c| {
            repo::connections::set_secret_ref(c, &placeholder.id, Some("mystique-db/connection/3"))
        })
        .unwrap();

    let filled = app.attach_session_secrets(open_req(&plain)).unwrap();
    assert_eq!(filled.password.as_deref(), Some("plain-pw"));
    assert_eq!(refs(&app, &plain.id).0.as_deref(), Some(VAULT_SECRET_REF));
    let filled = app.attach_session_secrets(open_req(&placeholder)).unwrap();
    assert!(filled.password.is_none());
    assert!(refs(&app, &placeholder.id).0.is_none());
    assert_eq!(items.total_reads(), 0);
    assert_eq!(keys.loads(), 1);
}

#[test]
fn file_mode_never_touches_old_items() {
    let app = AppServices::with_secrets(
        AppDatabase::open_in_memory().unwrap(),
        ConnectionManager::bootstrap(),
        SecretStore::from_parts(Arc::new(crate::secrets::MemoryKeySource::new()), None),
    );
    let mut bare = pg_input("old");
    bare.password = None;
    let profile = app.profiles().save(bare).unwrap();
    app.db
        .with_conn(|c| {
            repo::connections::set_secret_ref(c, &profile.id, Some(&secret_account(&profile.id)))
        })
        .unwrap();
    let err = app.attach_session_secrets(open_req(&profile)).unwrap_err();
    assert!(
        err.to_string().contains("MYSTIQUE_VAULT_KEY_SOURCE"),
        "{err}"
    );
    // New saves still work.
    let saved = app.profiles().save(pg_input("new")).unwrap();
    let filled = app.attach_session_secrets(open_req(&saved)).unwrap();
    assert_eq!(filled.password.as_deref(), Some("pg-pw"));
}

#[tokio::test]
async fn a_live_tool_session_binds_no_secret() {
    let file = TempDb::new("tool-db");
    let data = TempDb::new("tool-data");
    let (keys, items) = fakes();
    let app = app_on(file.open(), &keys, &items);
    let mut input = pg_input("sqlite");
    input.engine = "sqlite".into();
    input.plugin_config = json!({"path": data.0.to_string_lossy()});
    input.password = Some("unused".into());
    let profile = app.profiles().save(input).unwrap();
    let tools = app.agent_tools();
    app.secrets.forget_cached();
    keys.reset();

    tools
        .ensure_session_for(&profile.id, ToolCaller::Agent)
        .await
        .unwrap();
    assert_eq!(keys.loads(), 1);

    app.secrets.forget_cached();
    keys.reset();
    tools
        .ensure_session_for(&profile.id, ToolCaller::Agent)
        .await
        .unwrap();
    assert_eq!(keys.loads(), 0, "a live session needs no secret");
    assert_eq!(items.total_reads(), 0);
    let _ = app
        .connections
        .close_session(&format!("agent:{}", profile.id))
        .await;
}

#[tokio::test]
async fn mcp_never_prompts_for_a_locked_vault() {
    let file = TempDb::new("mcp-db");
    let data = TempDb::new("mcp-data");
    let (keys, items) = fakes();
    let profile = {
        let app = app_on(file.open(), &keys, &items);
        let mut input = pg_input("sqlite");
        input.engine = "sqlite".into();
        input.plugin_config = json!({"path": data.0.to_string_lossy()});
        input.password = Some("unused".into());
        app.profiles().save(input).unwrap()
    };
    // A new process where reading the key would need a prompt.
    keys.needs_ui.store(true, Ordering::SeqCst);
    let app = app_on(file.open(), &keys, &items);
    let tools = app.agent_tools();
    let err = tools
        .ensure_session_for(&profile.id, ToolCaller::Mcp)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Saved passwords are locked"),
        "{err}"
    );
    assert_eq!(app.secrets.status().state, "idle", "not cached as a denial");

    // Once the app unlocks (it may prompt), MCP uses the cached key.
    tools
        .ensure_session_for(&profile.id, ToolCaller::Agent)
        .await
        .unwrap();
    tools
        .ensure_session_for(&profile.id, ToolCaller::Mcp)
        .await
        .unwrap();
    for sid in ["agent", "mcp"] {
        let _ = app
            .connections
            .close_session(&format!("{sid}:{}", profile.id))
            .await;
    }
}

#[test]
fn mongo_username_lives_in_plugin_config() {
    let (keys, items) = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys, &items);
    let saved = app.profiles().save(mongo_input()).unwrap();
    assert_eq!(saved.plugin_config["username"], "admin");
    assert_eq!(refs(&app, &saved.id).0.as_deref(), Some(VAULT_SECRET_REF));

    app.secrets.forget_cached();
    keys.reset();
    let mut req = open_req(&saved);
    req.plugin_config = json!({"uri": "mongodb://127.0.0.1:27017/app?authSource=admin"});
    let filled = app.attach_session_secrets(req).unwrap();
    assert_eq!(filled.plugin_config["username"], "admin");
    assert_eq!(filled.password.as_deref(), Some("s@cret"));
    assert_eq!(keys.loads(), 1);
    assert_eq!(items.total_reads(), 0);
}

#[test]
fn editing_a_mongo_profile_keeps_the_username() {
    let (keys, items) = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys, &items);
    let saved = app.profiles().save(mongo_input()).unwrap();
    // The form sends back the stripped URI and no username.
    let mut update = mongo_input();
    update.id = Some(saved.id.clone());
    update.name = "renamed".into();
    update.plugin_config = json!({"uri": "mongodb://127.0.0.1:27017/app?authSource=admin"});
    let updated = app.profiles().save(update).unwrap();
    assert_eq!(updated.plugin_config["username"], "admin");
}

#[test]
fn legacy_mongo_user_item_moves_to_plugin_config_on_first_connect() {
    let (keys, items) = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys, &items);
    let mut input = mongo_input();
    input.plugin_config = json!({"uri": "mongodb://127.0.0.1:27017/app?authSource=admin"});
    input.password = Some("s@cret".into());
    // How an older build left it: no username in the row, `.user` beside the password.
    let saved = phase_a_row(&app, &items, input);
    let pointer = refs(&app, &saved.id).0.unwrap();
    let user_account = user_secret_account(&pointer);
    items.plant(&user_account, "admin");
    items.reset_reads();

    let filled = app.attach_session_secrets(open_req(&saved)).unwrap();
    assert_eq!(filled.plugin_config["username"], "admin");
    assert_eq!(filled.password.as_deref(), Some("s@cret"));
    assert_eq!(items.reads_of(&user_account), 1);
    assert!(
        !items.contains(&user_account),
        "item deleted after the move"
    );
    assert!(!items.contains(&pointer));
    assert_eq!(
        app.profiles().get(&saved.id).unwrap().plugin_config["username"],
        "admin"
    );

    app.secrets.forget_cached();
    let again = app.attach_session_secrets(open_req(&saved)).unwrap();
    assert_eq!(again.plugin_config["username"], "admin");
    assert_eq!(items.reads_of(&user_account), 1, "no second read");
}

#[test]
fn orphaned_items_are_purged_without_touching_live_ones() {
    let (keys, items) = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys, &items);
    let install = install_of(&app);
    let live = phase_a_row(&app, &items, pg_input("still-old"));
    let live_account = scoped_secret_account(&install, &live.id);
    let gone_id = "999";
    let orphans = [
        format!("connection.{}.password", uuid::Uuid::new_v4()),
        "ai.api_key.google".to_string(),
        scoped_secret_account(&install, gone_id),
        scoped_tls_secret_account(&install, gone_id),
        format!("conn.{}.4", uuid::Uuid::new_v4()),
    ];
    for account in &orphans {
        items.plant(account, "x");
    }
    items.plant(crate::secrets::VAULT_ACCOUNT, "key");
    items.plant("someone.else", "x");
    items.reset_reads();

    let report = app.vault().purge_keychain().unwrap();
    assert!(report.supported);
    assert_eq!(report.deleted, orphans.len());
    for account in &orphans {
        assert!(!items.contains(account), "{account}");
    }
    assert!(items.contains(&live_account), "a row still points at it");
    assert!(items.contains(crate::secrets::VAULT_ACCOUNT));
    assert!(items.contains("someone.else"));
    assert_eq!(items.total_reads(), 0, "listing reads no secret");
    assert_eq!(keys.loads(), 0);

    // The row still moves on its next connect.
    let filled = app.attach_session_secrets(open_req(&live)).unwrap();
    assert_eq!(filled.password.as_deref(), Some("pg-pw"));
}

#[test]
fn saved_ssh_secret_only_goes_to_the_saved_bastion() {
    let (keys, items) = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys, &items);
    let profile = full_pg(&app);
    let tunnel = |host: &str, port: u16, user: &str, auth: SshAuth| SshTunnelRequest {
        host: host.into(),
        port,
        user: user.into(),
        auth,
        password: None,
        private_key_path: None,
        passphrase: None,
        trusted_fingerprint: None,
    };
    let bound = |ssh: SshTunnelRequest| {
        let mut req = open_req(&profile);
        req.ssh = Some(ssh);
        let ssh = app.attach_session_secrets(req).unwrap().ssh.unwrap();
        ssh.password.or(ssh.passphrase)
    };
    assert_eq!(
        bound(tunnel("BASTION.example", 22, "ubuntu", SshAuth::Password)).as_deref(),
        Some("ssh-pw")
    );
    assert!(bound(tunnel("evil.example", 22, "ubuntu", SshAuth::Password)).is_none());
    assert!(bound(tunnel("bastion.example", 2222, "ubuntu", SshAuth::Password)).is_none());
    assert!(bound(tunnel("bastion.example", 22, "root", SshAuth::Password)).is_none());
    assert!(bound(tunnel("bastion.example", 22, "ubuntu", SshAuth::Key)).is_none());

    // The saved DB password follows the same rule.
    let db_password = |ssh: SshTunnelRequest| {
        let mut req = open_req(&profile);
        req.ssh = Some(ssh);
        app.attach_session_secrets(req).unwrap().password
    };
    assert!(db_password(tunnel("bastion.example", 22, "ubuntu", SshAuth::Password)).is_some());
    assert!(db_password(tunnel("evil.example", 22, "ubuntu", SshAuth::Password)).is_none());
}

#[test]
fn environment_and_read_only_come_from_the_profile() {
    let (keys, items) = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys, &items);
    let mut input = pg_input("prod");
    input.environment = "production".into();
    input.read_only = true;
    let profile = app.profiles().save(input).unwrap();
    let mut req = open_req(&profile);
    req.environment = Some("local".into());
    req.profile_read_only = false;
    let filled = app.attach_session_secrets(req).unwrap();
    assert_eq!(filled.environment.as_deref(), Some("production"));
    assert!(filled.profile_read_only);
}

#[test]
fn tls_server_name_redirect_gets_no_saved_password() {
    let (keys, items) = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys, &items);
    let profile = app.profiles().save(pg_input("pg")).unwrap();
    app.secrets.forget_cached();
    keys.reset();
    let mut req = open_req(&profile);
    req.plugin_config["tlsServerName"] = json!("evil.example");
    let filled = app.attach_session_secrets(req).unwrap();
    assert!(filled.password.is_none());
    assert_eq!(keys.loads(), 0);
    assert_eq!(items.total_reads(), 0);
}
