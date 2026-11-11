//! Keychain access counts (K-01…K-04) under the vault, and host binding
//! (R-06, R-07, R-16). A counting key source stands in for the one vault
//! item; a fake keychain stands in for items older builds left behind.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::domain::{SaveConnectionInput, SshAuth, SshConfig, SshTunnelRequest, VAULT_SECRET_REF};
use crate::secrets::counting::{CountingKeySource, FakeKeychain};
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

/// One process: a fresh vault over a shared key source.
fn app_on(db: AppDatabase, keys: &Arc<CountingKeySource>) -> AppServices {
    AppServices::with_secrets(
        db,
        ConnectionManager::bootstrap(),
        SecretStore::with_key_source(keys.clone()),
    )
}

fn fakes() -> Arc<CountingKeySource> {
    Arc::new(CountingKeySource::default())
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

#[test]
fn startup_loads_no_key() {
    let file = TempDb::new("boot");
    let keys = fakes();
    {
        let app = app_on(file.open(), &keys);
        full_pg(&app);
        app.profiles().save(pg_input("pw-only")).unwrap();
        app.profiles().save(mongo_input()).unwrap();
    }
    keys.reset();

    // A cold launch: construct the services and list the profiles.
    let app = app_on(file.open(), &keys);
    assert_eq!(app.profiles().list().unwrap().len(), 3);
    assert_eq!(keys.loads(), 0, "startup loaded the vault key");
    assert_eq!(app.secrets.status().state, "idle");
}

#[test]
fn connect_loads_the_vault_key_once_even_in_parallel() {
    let file = TempDb::new("connect");
    let keys = fakes();
    let (profile, other) = {
        let app = app_on(file.open(), &keys);
        (
            full_pg(&app),
            app.profiles().save(pg_input("other")).unwrap(),
        )
    };
    // A new process, whose key source is slow so first uses overlap.
    let slow = Arc::new(CountingKeySource::slow(Duration::from_millis(40)));
    let key = keys.load(crate::secrets::Prompt::Allowed).unwrap();
    slow.seed(key);
    let app = Arc::new(app_on(file.open(), &slow));
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

    // Later connects in the same process read nothing.
    app.attach_session_secrets(open_req(&profile)).unwrap();
    assert_eq!(slow.loads(), 1);
}

#[test]
fn a_profile_without_saved_secrets_loads_no_key() {
    let keys = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys);
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

#[tokio::test]
async fn a_live_tool_session_binds_no_secret() {
    let file = TempDb::new("tool-db");
    let data = TempDb::new("tool-data");
    let keys = fakes();
    let app = app_on(file.open(), &keys);
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
    let _ = app
        .connections
        .close_session(&format!("agent:{}", profile.id))
        .await;
}

#[tokio::test]
async fn mcp_never_prompts_for_a_locked_vault() {
    let file = TempDb::new("mcp-db");
    let data = TempDb::new("mcp-data");
    let keys = fakes();
    let profile = {
        let app = app_on(file.open(), &keys);
        let mut input = pg_input("sqlite");
        input.engine = "sqlite".into();
        input.plugin_config = json!({"path": data.0.to_string_lossy()});
        input.password = Some("unused".into());
        app.profiles().save(input).unwrap()
    };
    // A new process where reading the key would need a prompt.
    keys.needs_ui.store(true, Ordering::SeqCst);
    let app = app_on(file.open(), &keys);
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
    let keys = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys);
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
}

#[test]
fn editing_a_mongo_profile_keeps_the_username() {
    let keys = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys);
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
fn orphaned_items_are_purged_except_the_vault_key() {
    let keys = fakes();
    let items = Arc::new(FakeKeychain::default());
    let app = AppServices::with_secrets(
        AppDatabase::open_in_memory().unwrap(),
        ConnectionManager::bootstrap(),
        SecretStore::from_parts(keys.clone(), Some(items.clone())),
    );
    let live = app.profiles().save(pg_input("live")).unwrap();
    let install = uuid::Uuid::new_v4();
    let orphans = [
        format!("connection.{}.password", uuid::Uuid::new_v4()),
        "ai.api_key.google".to_string(),
        format!("conn.{}", live.id),
        format!("conn.{}.tls", live.id),
        format!("conn.{install}.{}", live.id),
        format!("conn.{install}.{}.ssh", live.id),
        format!("conn.{install}.{}.user", live.id),
    ];
    for account in &orphans {
        items.plant(account, "x");
    }
    items.plant(crate::secrets::VAULT_ACCOUNT, "key");
    items.plant("someone.else", "x");
    keys.reset();

    let report = app.vault().purge_keychain().unwrap();
    assert!(report.supported);
    assert_eq!(report.deleted, orphans.len());
    assert_eq!(report.skipped, 0);
    for account in &orphans {
        assert!(!items.contains(account), "{account}");
    }
    assert!(items.contains(crate::secrets::VAULT_ACCOUNT));
    assert!(items.contains("someone.else"));
    assert_eq!(keys.loads(), 0);

    // The saved password lives in the vault, not in a purged item.
    let filled = app.attach_session_secrets(open_req(&live)).unwrap();
    assert_eq!(filled.password.as_deref(), Some("pg-pw"));
}

#[test]
fn saved_ssh_secret_only_goes_to_the_saved_bastion() {
    let keys = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys);
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
    let keys = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys);
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
    let keys = fakes();
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &keys);
    let profile = app.profiles().save(pg_input("pg")).unwrap();
    app.secrets.forget_cached();
    keys.reset();
    let mut req = open_req(&profile);
    req.plugin_config["tlsServerName"] = json!("evil.example");
    let filled = app.attach_session_secrets(req).unwrap();
    assert!(filled.password.is_none());
    assert_eq!(keys.loads(), 0);
}
