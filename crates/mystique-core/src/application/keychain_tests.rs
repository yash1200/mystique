//! Keychain read counts (K-01…K-04) and host binding (R-06, R-07, R-16),
//! against a fake backend that counts OS reads per account.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::domain::{
    scoped_secret_account, scoped_ssh_secret_account, scoped_tls_secret_account,
    user_secret_account, SaveConnectionInput, SshAuth, SshConfig, SshTunnelRequest,
};
use crate::secrets::counting::CountingBackend;
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

fn app_on(db: AppDatabase, backend: &Arc<CountingBackend>) -> AppServices {
    AppServices::with_secrets(
        db,
        ConnectionManager::bootstrap(),
        SecretStore::from_backend(backend.clone()),
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
    }
}

/// Password, bastion password, and an encrypted TLS client key.
fn full_pg(app: &AppServices) -> crate::domain::ConnectionProfile {
    let mut input = pg_input("full");
    input.plugin_config = json!({
        "host": "db.internal",
        "port": 5432,
        "database": "app",
        "sslMode": "verify-full",
        "clientCertPath": "/certs/client.crt",
        "clientKeyPath": "/certs/client.key"
    });
    input.ssh_config = SshConfig {
        enabled: true,
        host: "bastion.example".into(),
        port: 22,
        user: "ubuntu".into(),
        auth: SshAuth::Password,
        private_key_path: String::new(),
    };
    input.ssh_password = Some("ssh-pw".into());
    input.tls_key_passphrase = Some("tls-pp".into());
    app.profiles().save(input).unwrap()
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
    app.db
        .with_conn(|c| repo::settings::get(c, "install_id"))
        .unwrap()
        .unwrap()
        .value
}

#[test]
fn startup_reads_no_keychain_item() {
    let file = TempDb::new("boot");
    let backend = Arc::new(CountingBackend::default());
    let (plaintext_id, placeholder_id) = {
        let app = app_on(file.open(), &backend);
        full_pg(&app);
        app.profiles().save(pg_input("pw-only")).unwrap();
        app.profiles().save(mongo_input()).unwrap();
        let legacy = app.profiles().save(pg_input("legacy")).unwrap();
        // Older rows: a `conn.<id>` pointer, plaintext, and a placeholder.
        app.secrets.set(&legacy.id, "legacy-pw").unwrap();
        app.db
            .with_conn(|c| {
                repo::connections::set_secret_ref(
                    c,
                    &legacy.id,
                    Some(&crate::domain::secret_account(&legacy.id)),
                )
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
        (plain.id, placeholder.id)
    };
    backend.reset_reads();

    // A cold launch: construct the services, then run the startup cleanup.
    let app = app_on(file.open(), &backend);
    app.migrate_legacy_secrets().unwrap();
    assert_eq!(
        backend.total_reads(),
        0,
        "startup read {:?}",
        backend.read_accounts()
    );
    let install = install_of(&app);
    let plain_ref = app
        .db
        .with_conn(|c| repo::connections::get_secret_ref(c, &plaintext_id))
        .unwrap();
    assert_eq!(
        plain_ref.as_deref(),
        Some(scoped_secret_account(&install, &plaintext_id).as_str())
    );
    assert!(app
        .db
        .with_conn(|c| repo::connections::get_secret_ref(c, &placeholder_id))
        .unwrap()
        .is_none());

    // The flag makes it one-time: plaintext planted later is left for connect.
    app.db
        .with_conn(|c| repo::connections::set_secret_ref(c, &plaintext_id, Some("again")))
        .unwrap();
    app.migrate_legacy_secrets().unwrap();
    assert_eq!(
        app.db
            .with_conn(|c| repo::connections::get_secret_ref(c, &plaintext_id))
            .unwrap()
            .as_deref(),
        Some("again")
    );
    assert_eq!(backend.total_reads(), 0);
}

#[test]
fn connect_reads_each_item_once_even_in_parallel() {
    let file = TempDb::new("connect");
    let backend = Arc::new(CountingBackend::default());
    let (profile, install) = {
        let app = app_on(file.open(), &backend);
        (full_pg(&app), install_of(&app))
    };
    let slow = Arc::new(CountingBackend::slow(Duration::from_millis(40)));
    for account in [
        scoped_secret_account(&install, &profile.id),
        scoped_ssh_secret_account(&install, &profile.id),
        scoped_tls_secret_account(&install, &profile.id),
    ] {
        let value = SecretStore::from_backend(backend.clone())
            .get_named(&account)
            .unwrap()
            .unwrap();
        crate::secrets::SecretBackend::set(slow.as_ref(), &account, &value).unwrap();
    }

    let app = Arc::new(app_on(file.open(), &slow));
    let handles: Vec<_> = (0..6)
        .map(|_| {
            let app = Arc::clone(&app);
            let req = open_req(&profile);
            std::thread::spawn(move || app.attach_session_secrets(req).unwrap())
        })
        .collect();
    for handle in handles {
        let filled = handle.join().unwrap();
        assert_eq!(filled.password.as_deref(), Some("pg-pw"));
        assert_eq!(filled.tls_key_passphrase.as_deref(), Some("tls-pp"));
        let ssh = filled.ssh.expect("saved tunnel");
        assert_eq!(ssh.password.as_deref(), Some("ssh-pw"));
    }
    assert_eq!(
        slow.reads_of(&scoped_secret_account(&install, &profile.id)),
        1
    );
    assert_eq!(
        slow.reads_of(&scoped_ssh_secret_account(&install, &profile.id)),
        1
    );
    assert_eq!(
        slow.reads_of(&scoped_tls_secret_account(&install, &profile.id)),
        1
    );
    assert_eq!(slow.total_reads(), 3, "{:?}", slow.read_accounts());
}

#[test]
fn tls_is_not_read_without_a_saved_passphrase() {
    let backend = Arc::new(CountingBackend::default());
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &backend);
    let plain = app.profiles().save(pg_input("no-key")).unwrap();
    let mut keyed = pg_input("key-no-pass");
    keyed.plugin_config["clientKeyPath"] = json!("/certs/client.key");
    let keyed = app.profiles().save(keyed).unwrap();
    assert_eq!(
        keyed.plugin_config["clientKeyPassphraseSaved"],
        json!(false)
    );
    backend.reset_reads();
    for profile in [&plain, &keyed] {
        let filled = app.attach_session_secrets(open_req(profile)).unwrap();
        assert!(filled.tls_key_passphrase.is_none());
    }
    let install = install_of(&app);
    for profile in [&plain, &keyed] {
        assert_eq!(
            backend.reads_of(&scoped_tls_secret_account(&install, &profile.id)),
            0
        );
    }

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
    let backend = Arc::new(CountingBackend::default());
    let app = app_on(file.open(), &backend);
    let mut input = pg_input("sqlite");
    input.engine = "sqlite".into();
    input.plugin_config = json!({"path": data.0.to_string_lossy()});
    input.password = Some("unused".into());
    let profile = app.profiles().save(input).unwrap();
    let tools = app.agent_tools();
    app.secrets.forget_cached();
    backend.reset_reads();

    tools
        .ensure_session_for(&profile.id, ToolCaller::Agent)
        .await
        .unwrap();
    assert_eq!(backend.total_reads(), 1, "{:?}", backend.read_accounts());

    app.secrets.forget_cached();
    backend.reset_reads();
    tools
        .ensure_session_for(&profile.id, ToolCaller::Agent)
        .await
        .unwrap();
    assert_eq!(backend.total_reads(), 0, "{:?}", backend.read_accounts());
    let _ = app
        .connections
        .close_session(&format!("agent:{}", profile.id))
        .await;
}

#[test]
fn mongo_username_lives_in_plugin_config() {
    let backend = Arc::new(CountingBackend::default());
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &backend);
    let saved = app.profiles().save(mongo_input()).unwrap();
    assert_eq!(saved.plugin_config["username"], "admin");
    let pointer = app
        .db
        .with_conn(|c| repo::connections::get_secret_ref(c, &saved.id))
        .unwrap()
        .unwrap();
    assert!(!backend.contains(&user_secret_account(&pointer)));

    app.secrets.forget_cached();
    backend.reset_reads();
    let mut req = open_req(&saved);
    req.plugin_config = json!({"uri": "mongodb://127.0.0.1:27017/app?authSource=admin"});
    let filled = app.attach_session_secrets(req).unwrap();
    assert_eq!(filled.plugin_config["username"], "admin");
    assert_eq!(filled.password.as_deref(), Some("s@cret"));
    assert_eq!(backend.total_reads(), 1, "{:?}", backend.read_accounts());
}

#[test]
fn editing_a_mongo_profile_keeps_the_username() {
    let backend = Arc::new(CountingBackend::default());
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &backend);
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
    let backend = Arc::new(CountingBackend::default());
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &backend);
    let saved = app.profiles().save(mongo_input()).unwrap();
    let pointer = app
        .db
        .with_conn(|c| repo::connections::get_secret_ref(c, &saved.id))
        .unwrap()
        .unwrap();
    // How an older build left it: no username in the row, a `.user` item.
    let user_account = user_secret_account(&pointer);
    let mut cfg = saved.plugin_config.clone();
    cfg.as_object_mut().unwrap().remove("username");
    app.db
        .with_conn(|c| repo::connections::set_plugin_config(c, &saved.id, &cfg))
        .unwrap();
    app.secrets.set_named(&user_account, "admin").unwrap();
    app.secrets.forget_cached();
    backend.reset_reads();

    let mut legacy = saved.clone();
    legacy.plugin_config = cfg;
    let filled = app.attach_session_secrets(open_req(&legacy)).unwrap();
    assert_eq!(filled.plugin_config["username"], "admin");
    assert_eq!(backend.reads_of(&user_account), 1);
    assert!(
        !backend.contains(&user_account),
        "item deleted after the move"
    );
    assert_eq!(
        app.profiles().get(&saved.id).unwrap().plugin_config["username"],
        "admin"
    );

    app.secrets.forget_cached();
    let again = app.attach_session_secrets(open_req(&legacy)).unwrap();
    assert_eq!(again.plugin_config["username"], "admin");
    assert_eq!(backend.reads_of(&user_account), 1, "no second read");
}

#[test]
fn saved_ssh_secret_only_goes_to_the_saved_bastion() {
    let backend = Arc::new(CountingBackend::default());
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &backend);
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
}

#[test]
fn environment_and_read_only_come_from_the_profile() {
    let backend = Arc::new(CountingBackend::default());
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &backend);
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
    let backend = Arc::new(CountingBackend::default());
    let app = app_on(AppDatabase::open_in_memory().unwrap(), &backend);
    let profile = app.profiles().save(pg_input("pg")).unwrap();
    backend.reset_reads();
    let mut req = open_req(&profile);
    req.plugin_config["tlsServerName"] = json!("evil.example");
    let filled = app.attach_session_secrets(req).unwrap();
    assert!(filled.password.is_none());
    assert_eq!(backend.total_reads(), 0);
}
