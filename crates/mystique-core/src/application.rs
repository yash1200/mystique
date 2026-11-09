//! Application services: use-cases on top of storage + connectors.
//!
//! IPC / HTTP adapters call these — never repositories or drivers directly.

mod agent_chat;
mod agent_tools;
mod ai_secrets;
pub(crate) mod dbml;
mod grid_layout;
mod history;
mod profiles;
mod query_library;
mod settings;
mod vault;
mod workspace_tabs;

pub use agent_chat::AgentChatService;
pub use agent_tools::{
    parse_allowlist, tool_specs, tool_specs_for, AgentColumnInfo, AgentConnectionInfo,
    AgentObjectInfo, AgentQueryDownload, AgentToolSpec, AgentTools, AgentWritePolicy, ToolCaller,
    AGENT_DEFAULT_CELL_CHARS, AGENT_MAX_CELL_CHARS, AGENT_MAX_ROWS, SETTING_MCP_ALLOWLIST,
    SETTING_MCP_POLICY,
};
pub use ai_secrets::AiSecretsService;
pub(crate) use grid_layout::DbGridSaveStore;
pub use grid_layout::GridLayoutService;
pub use history::HistoryService;
pub use profiles::ProfileService;
pub use query_library::QueryLibraryService;
pub use settings::SettingsService;
pub use vault::{PurgeReport, VaultService};
pub use workspace_tabs::WorkspaceTabsService;

use std::path::Path;
use std::sync::Arc;

use mystique_connector_api::{SessionInfo, TestResult};

use crate::connectors::{ConnectionManager, OpenSessionRequest};
use crate::error::AppResult;
use crate::secrets::{PendingPassword, Prompt, SecretStore};
use crate::storage::AppDatabase;

/// Bundle of application services.
///
/// Held in Tauri state (or tests) as `Arc<AppServices>`.
pub struct AppServices {
    pub db: Arc<AppDatabase>,
    pub connections: Arc<ConnectionManager>,
    pub secrets: Arc<SecretStore>,
    /// Last tool results, so cell paging never re-runs a statement.
    tool_results: Arc<agent_tools::ToolResultCache>,
}

impl AppServices {
    /// The app. The vault key source follows the build (see `secrets::vault`);
    /// `data_dir` holds the debug key file.
    pub fn new(db: AppDatabase, data_dir: &Path) -> Self {
        let app = Self::with_secrets(
            db,
            ConnectionManager::bootstrap(),
            SecretStore::os(data_dir),
        );
        app.maintain_after_first_unlock();
        app
    }

    pub fn with_secrets(
        db: AppDatabase,
        connections: ConnectionManager,
        secrets: SecretStore,
    ) -> Self {
        let db = Arc::new(db);
        let connections = Arc::new(connections);
        history::install_writer(&connections, Arc::clone(&db));
        connections.set_grid_save_store(Arc::new(DbGridSaveStore {
            db: Arc::clone(&db),
        }));
        let reaper = Arc::clone(&connections);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    reaper.reap_idle().await;
                }
            });
        }
        Self {
            db,
            connections,
            secrets: Arc::new(secrets),
            tool_results: Arc::new(agent_tools::ToolResultCache::default()),
        }
    }

    /// One-time startup cleanup of placeholder pointers.
    ///
    /// Not part of construction: the shell runs it off the main thread. It
    /// loads no vault key and reads no keychain item.
    pub fn migrate_legacy_secrets(&self) -> AppResult<()> {
        self.profiles().startup_cleanup()
    }

    /// Once the key first loads, in the background: seal leftover plaintext
    /// rows, then (once per install) delete orphaned keychain items.
    fn maintain_after_first_unlock(&self) {
        let db = Arc::downgrade(&self.db);
        let secrets = Arc::downgrade(&self.secrets);
        self.secrets.on_first_unlock(move || {
            let (db, secrets) = (db.clone(), secrets.clone());
            std::thread::spawn(move || {
                if let (Some(db), Some(secrets)) = (db.upgrade(), secrets.upgrade()) {
                    vault::after_first_unlock(&db, &secrets);
                }
            });
        });
    }

    pub fn agent_tools(&self) -> AgentTools {
        AgentTools::new(
            Arc::clone(&self.db),
            Arc::clone(&self.connections),
            Arc::clone(&self.secrets),
        )
        .with_result_cache(Arc::clone(&self.tool_results))
    }

    pub fn profiles(&self) -> ProfileService<'_> {
        ProfileService {
            db: &self.db,
            secrets: &self.secrets,
            prompt: Prompt::Allowed,
        }
    }

    /// App-level secrets (AI keys, the MCP token) and the vault itself.
    pub fn vault(&self) -> VaultService<'_> {
        VaultService {
            db: &self.db,
            secrets: &self.secrets,
            prompt: Prompt::Allowed,
        }
    }

    /// Fill DB password + SSH tunnel from the profile / keychain / known_hosts.
    ///
    /// A saved profile's secret is only attached when the client is dialing
    /// that profile's host. Database name and SSL fields may still change.
    /// Environment and read-only always come from the saved profile.
    pub fn attach_session_secrets(&self, req: OpenSessionRequest) -> AppResult<OpenSessionRequest> {
        bind_open_request(&self.db, &self.secrets, req, Prompt::Allowed)
    }

    pub fn lookup_ssh_host_key(
        &self,
        host: &str,
        port: u16,
    ) -> AppResult<Option<crate::storage::sqlite::repo::ssh_hosts::KnownHost>> {
        self.db
            .with_conn(|c| crate::storage::sqlite::repo::ssh_hosts::lookup(c, host, port))
    }

    pub fn trust_ssh_host_key(
        &self,
        host: mystique_connector_api::SshHostKeyPrompt,
    ) -> AppResult<()> {
        let row = crate::storage::sqlite::repo::ssh_hosts::KnownHost {
            host: host.host,
            port: host.port,
            key_type: host.key_type,
            fingerprint: host.fingerprint,
            public_key: host.public_key,
        };
        self.db
            .with_conn(|c| crate::storage::sqlite::repo::ssh_hosts::upsert(c, &row))
    }

    /// Bind secrets and open the desktop session. A typed password for a
    /// profile that does not save it is kept for the app session once the
    /// dial works.
    pub async fn open_session(&self, req: OpenSessionRequest) -> AppResult<SessionInfo> {
        let (req, session) = self.bind_off_thread(req).await?;
        self.load_database_marks(&req);
        let result = self.connections.open_session(req).await;
        settle_session_password(&self.secrets, session, &result);
        Ok(result?)
    }

    /// The saved profile's per-database marks reach the manager before its
    /// session opens, so the first write is judged with them.
    fn load_database_marks(&self, req: &OpenSessionRequest) {
        let Some(id) = req
            .profile_id
            .as_deref()
            .or(req.session_id.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return;
        };
        if let Ok(profile) = self
            .db
            .with_conn(|c| crate::storage::sqlite::repo::connections::get_by_id(c, id))
        {
            self.connections
                .set_database_marks(&profile.id, profile.database_environments);
        }
    }

    /// Mark one database of a profile (or clear its mark). Open sessions of
    /// the profile use the new marks from their next write.
    pub fn set_database_environment(
        &self,
        input: &crate::domain::SetDatabaseEnvironmentInput,
    ) -> AppResult<crate::domain::ConnectionProfile> {
        let profile = self.profiles().set_database_environment(input)?;
        self.connections
            .set_database_marks(&profile.id, profile.database_environments.clone());
        Ok(profile)
    }

    /// Bind secrets and test. A typed password that passes is kept, as for
    /// a connect.
    pub async fn test_connection(&self, req: OpenSessionRequest) -> AppResult<TestResult> {
        let (req, session) = self.bind_off_thread(req).await?;
        let result = self.connections.test_connection(&req).await;
        let outcome = match &result {
            Ok(test) if test.ok => Ok(()),
            Ok(test) => Err(test.message.clone()),
            Err(e) => Err(e.to_string()),
        };
        settle_session_password(&self.secrets, session, &outcome);
        Ok(result?)
    }

    /// The vault key may load here: off the async workers.
    async fn bind_off_thread(
        &self,
        req: OpenSessionRequest,
    ) -> AppResult<(OpenSessionRequest, SessionUse)> {
        let (db, secrets) = (Arc::clone(&self.db), Arc::clone(&self.secrets));
        tokio::task::spawn_blocking(move || bind_with_session(&db, &secrets, req, Prompt::Allowed))
            .await
            .map_err(|e| crate::error::AppError::Internal(format!("secret binding: {e}")))?
    }

    /// Drop every session password. The app calls this on quit.
    pub fn forget_session_passwords(&self) {
        self.secrets.session().clear();
    }

    /// Typed request password wins. A saved secret is attached only for that profile's host.
    pub fn password_for_request(&self, req: &OpenSessionRequest) -> AppResult<Option<String>> {
        Ok(bind_open_request(&self.db, &self.secrets, req.clone(), Prompt::Allowed)?.password)
    }

    pub fn query_library(&self) -> QueryLibraryService<'_> {
        QueryLibraryService { db: &self.db }
    }

    pub fn workspace_tabs(&self) -> WorkspaceTabsService<'_> {
        WorkspaceTabsService { db: &self.db }
    }

    pub fn grid_layout(&self) -> GridLayoutService<'_> {
        GridLayoutService { db: &self.db }
    }

    pub fn history(&self) -> HistoryService<'_> {
        HistoryService { db: &self.db }
    }

    pub fn settings(&self) -> SettingsService<'_> {
        SettingsService { db: &self.db }
    }

    pub fn agent_chats(&self) -> AgentChatService<'_> {
        AgentChatService { db: &self.db }
    }

    pub fn ai_secrets(&self) -> AiSecretsService<'_> {
        AiSecretsService { db: &self.db }
    }

    pub fn connections(&self) -> Arc<ConnectionManager> {
        Arc::clone(&self.connections)
    }
}

/// Saved secrets stay on the stored host. The client may override database and SSL.
///
/// MCP passes `Prompt::Never`: a locked vault fails instead of prompting.
pub(crate) fn bind_open_request(
    db: &AppDatabase,
    secrets: &SecretStore,
    req: OpenSessionRequest,
    prompt: Prompt,
) -> AppResult<OpenSessionRequest> {
    Ok(bind_with_session(db, secrets, req, prompt)?.0)
}

/// [`bind_open_request`], plus how the session password store was used.
pub(crate) fn bind_with_session(
    db: &AppDatabase,
    secrets: &SecretStore,
    mut req: OpenSessionRequest,
    prompt: Prompt,
) -> AppResult<(OpenSessionRequest, SessionUse)> {
    let profiles = ProfileService {
        db,
        secrets,
        prompt,
    };
    let profile_id = req
        .profile_id
        .as_deref()
        .or(req.session_id.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let profile = profile_id.as_deref().and_then(|id| {
        db.with_conn(|c| crate::storage::sqlite::repo::connections::get_by_id(c, id))
            .ok()
    });
    let mut allow_stored = true;
    if let Some(profile) = &profile {
        // The production guard keys off these. A client value never wins.
        req.environment = Some(profile.environment.clone());
        req.profile_read_only = profile.read_only;
        if req
            .connection_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
        {
            req.connection_name = Some(profile.name.clone());
        }
        if crate::domain::host_identity::same_connection_target(
            &profile.engine,
            &profile.plugin_config,
            &req.plugin_config,
        ) {
            req.plugin_config = crate::domain::host_identity::overlay_non_host(
                &profile.plugin_config,
                &req.plugin_config,
            );
        } else {
            allow_stored = false;
        }
        // A client tunnel to another bastion would carry the saved password
        // there. Only the saved tunnel, or none, gets stored secrets.
        if req
            .ssh
            .as_ref()
            .is_some_and(|ssh| !same_ssh_target(&profile.ssh, ssh))
        {
            allow_stored = false;
        }
    }

    lift_request_credentials(&mut req);
    let typed = nonempty(req.password.as_deref()).map(str::to_string);
    let mut session = SessionUse::None;
    // A profile that does not save its password: typed, else this session's.
    // Never the vault, never the keychain.
    let asks = profile
        .as_ref()
        .filter(|p| !p.save_password && allow_stored);
    if let (Some(profile), Some(id)) = (asks, profile_id.as_deref()) {
        match typed {
            Some(pw) => session = SessionUse::Typed(secrets.session().pending(id, &pw)),
            None => {
                let pw = secrets
                    .session()
                    .get(id)
                    .ok_or(crate::error::AppError::PasswordRequired)?;
                req.password = Some(pw);
                session = SessionUse::Held(id.to_string());
                inject_mongo_username(&profiles, &profile.engine, id, &mut req.plugin_config)?;
            }
        }
    } else if typed.is_none() && allow_stored {
        if let Some(id) = profile_id.as_deref() {
            req.password = profiles.resolve_password(id)?;
            if let Some(profile) = &profile {
                inject_mongo_username(&profiles, &profile.engine, id, &mut req.plugin_config)?;
            }
        }
    }
    if nonempty(req.password.as_deref()).is_none() && allow_stored {
        if let Some(profile) = &profile {
            if profile.password_set {
                return Err(crate::error::AppError::SecretStore(
                    "The saved password is missing. Edit the connection and enter it again.".into(),
                ));
            }
        }
    }

    if allow_stored {
        // The test form sets use_saved_ssh false and omits ssh when the box
        // is unticked. Open and the agent leave the default, which fills it.
        if req.ssh.is_none() && req.use_saved_ssh {
            if let Some(profile) = &profile {
                let secret = profile_id
                    .as_deref()
                    .and_then(|id| profiles.resolve_ssh_secret(id).ok().flatten());
                req.ssh = crate::domain::SshTunnelRequest::from_config(&profile.ssh, secret);
            }
        } else if let Some(ssh) = req.ssh.as_mut() {
            let needs_secret = match ssh.auth {
                crate::domain::SshAuth::Password => nonempty(ssh.password.as_deref()).is_none(),
                crate::domain::SshAuth::Key => nonempty(ssh.passphrase.as_deref()).is_none(),
                crate::domain::SshAuth::Agent => false,
            };
            // The saved secret only goes to the saved bastion and user.
            let same_bastion = profile
                .as_ref()
                .is_some_and(|profile| same_ssh_target(&profile.ssh, ssh));
            if needs_secret && same_bastion {
                if let Some(id) = profile_id.as_deref() {
                    if let Ok(Some(secret)) = profiles.resolve_ssh_secret(id) {
                        match ssh.auth {
                            crate::domain::SshAuth::Password => ssh.password = Some(secret),
                            crate::domain::SshAuth::Key => ssh.passphrase = Some(secret),
                            crate::domain::SshAuth::Agent => {}
                        }
                    }
                }
            }
        }
    }
    if let Some(ssh) = req.ssh.as_mut() {
        if ssh.trusted_fingerprint.is_none() {
            if let Ok(Some(known)) = db.with_conn(|c| {
                crate::storage::sqlite::repo::ssh_hosts::lookup(c, &ssh.host, ssh.port)
            }) {
                ssh.trusted_fingerprint = Some(known.fingerprint);
            }
        }
    }
    if allow_stored && nonempty(req.tls_key_passphrase.as_deref()).is_none() {
        if let Some(id) = profile_id.as_deref() {
            if let Ok(Some(pw)) = profiles.resolve_tls_secret(id) {
                req.tls_key_passphrase = Some(pw);
            }
        }
    }
    Ok((req, session))
}

/// How a bound request used the session password store.
#[derive(Debug)]
pub(crate) enum SessionUse {
    None,
    /// Typed for a profile that does not save it: kept once the dial works.
    Typed(PendingPassword),
    /// Read from the store for this profile id.
    Held(String),
}

/// After the dial: keep a typed password that worked, drop a held one the
/// server refused.
pub(crate) fn settle_session_password<T, E: std::fmt::Display>(
    secrets: &SecretStore,
    session: SessionUse,
    result: &Result<T, E>,
) {
    match (session, result) {
        (SessionUse::Typed(pending), Ok(_)) => secrets.session().commit(pending),
        (SessionUse::Held(id), Err(e)) if is_auth_failure(&e.to_string()) => {
            secrets.session().forget(&id)
        }
        _ => {}
    }
}

fn is_auth_failure(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    [
        "authentication failed",
        "access denied",
        "password authentication",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

/// Host, port, user, and auth kind all match the saved tunnel.
fn same_ssh_target(
    stored: &crate::domain::SshConfig,
    req: &crate::domain::SshTunnelRequest,
) -> bool {
    let req_port = if req.port == 0 { 22 } else { req.port };
    stored.is_active()
        && stored.host.trim().eq_ignore_ascii_case(req.host.trim())
        && stored.normalized_port() == req_port
        && stored.user.trim() == req.user.trim()
        && stored.auth == req.auth
}

/// A password sitting in `plugin_config` or in a Mongo URI is client-supplied
/// material for this dial, not a keychain read.
fn lift_request_credentials(req: &mut OpenSessionRequest) {
    if nonempty(req.password.as_deref()).is_none() {
        if let Some(pw) = req
            .plugin_config
            .get("password")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            req.password = Some(pw.to_string());
        }
    }
    if let Some(obj) = req.plugin_config.as_object_mut() {
        obj.remove("password");
        obj.remove("clientKeyPassphrase");
        obj.remove("client_key_passphrase");
    }
    let Some(uri) = req
        .plugin_config
        .get("uri")
        .and_then(|v| v.as_str())
        .map(str::to_string)
    else {
        return;
    };
    let Some(creds) = crate::domain::mongo_uri::strip_mongo_userinfo(&uri) else {
        return;
    };
    if let Some(obj) = req.plugin_config.as_object_mut() {
        obj.insert("uri".into(), serde_json::Value::String(creds.uri));
        let user_empty = nonempty(
            obj.get("username")
                .or_else(|| obj.get("user"))
                .and_then(|v| v.as_str()),
        )
        .is_none();
        if user_empty && !creds.username.is_empty() {
            obj.insert("username".into(), serde_json::Value::String(creds.username));
        }
    }
    if nonempty(req.password.as_deref()).is_none() && !creds.password.is_empty() {
        req.password = Some(creds.password);
    }
}

/// Older saves kept the Mongo username in a keychain item. The first
/// connect moves it into `plugin_config`; later connects read no item.
fn inject_mongo_username(
    profiles: &ProfileService<'_>,
    engine: &str,
    id: &str,
    config: &mut serde_json::Value,
) -> AppResult<()> {
    if mystique_connector_api::runtime_id(engine) != Some("mongodb") {
        return Ok(());
    }
    if nonempty(
        config
            .get("username")
            .or_else(|| config.get("user"))
            .and_then(|v| v.as_str()),
    )
    .is_some()
    {
        return Ok(());
    }
    let Some(user) = profiles.mongo_username(id)? else {
        return Ok(());
    };
    if let Some(obj) = config.as_object_mut() {
        obj.insert("username".into(), serde_json::Value::String(user));
    }
    Ok(())
}

#[cfg(test)]
mod keychain_tests;

#[cfg(test)]
mod session_password_tests;

#[cfg(test)]
mod attach_secrets_tests {
    use super::*;
    use crate::connectors::ConnectionManager;
    use crate::domain::SaveConnectionInput;
    use crate::storage::AppDatabase;
    use serde_json::json;

    fn svc() -> AppServices {
        AppServices::with_secrets(
            AppDatabase::open_in_memory().unwrap(),
            ConnectionManager::bootstrap(),
            SecretStore::memory(),
        )
    }

    fn save_pg(app: &AppServices, password: Option<&str>) -> crate::domain::ConnectionProfile {
        app.profiles()
            .save(SaveConnectionInput {
                id: None,
                name: "pg".into(),
                engine: "postgres".into(),
                color_id: "teal".into(),
                environment: "local".into(),
                read_only: true,
                notes: None,
                plugin_config: json!({"host": "127.0.0.1", "database": "postgres"}),
                password: password.map(|s| s.into()),
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
            })
            .unwrap()
    }

    fn open_req(profile_id: String, plugin_config: serde_json::Value) -> OpenSessionRequest {
        OpenSessionRequest {
            engine: "postgres".into(),
            plugin_config,
            password: None,
            pool: None,
            session_id: None,
            profile_id: Some(profile_id),
            ssh: None,
            tls_key_passphrase: None,
            environment: None,
            connection_name: None,
            profile_read_only: false,
            use_saved_ssh: true,
        }
    }

    #[test]
    fn unticked_ssh_does_not_reuse_the_saved_tunnel() {
        let app = svc();
        let saved = app
            .profiles()
            .save(SaveConnectionInput {
                id: None,
                name: "pg".into(),
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
                ssh_config: crate::domain::SshConfig {
                    enabled: true,
                    host: "bastion.example".into(),
                    port: 22,
                    user: "ubuntu".into(),
                    auth: crate::domain::SshAuth::Agent,
                    private_key_path: String::new(),
                },
                ssh_password: None,
                clear_ssh_secret: false,
                tls_key_passphrase: None,
                query_timeout_secs: None,
                save_password: None,
            })
            .unwrap();
        let mut unticked = open_req(saved.id.clone(), saved.plugin_config.clone());
        unticked.use_saved_ssh = false;
        let filled = app.attach_session_secrets(unticked).unwrap();
        assert!(filled.ssh.is_none());

        let saved_ssh = app
            .attach_session_secrets(open_req(saved.id, saved.plugin_config))
            .unwrap();
        assert_eq!(
            saved_ssh.ssh.as_ref().map(|ssh| ssh.host.as_str()),
            Some("bastion.example")
        );
    }

    #[test]
    fn attach_fills_password_from_the_vault() {
        let app = svc();
        let saved = save_pg(&app, Some("s3cret"));
        let filled = app
            .attach_session_secrets(open_req(saved.id.clone(), saved.plugin_config.clone()))
            .unwrap();
        assert_eq!(filled.password.as_deref(), Some("s3cret"));
        assert!(filled.ssh.is_none());
    }

    #[test]
    fn attach_errors_when_the_stored_password_is_missing() {
        let app = svc();
        let saved = save_pg(&app, Some("s3cret"));
        app.db
            .with_conn(|c| {
                crate::storage::sqlite::repo::secrets::delete(
                    c,
                    "connection",
                    &saved.id,
                    "password",
                )
            })
            .unwrap();
        let err = app
            .attach_session_secrets(open_req(saved.id, saved.plugin_config))
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("missing"),
            "expected a missing password, got {msg}"
        );
    }

    #[test]
    fn different_host_does_not_receive_the_stored_password() {
        let app = svc();
        let saved = save_pg(&app, Some("s3cret"));
        let filled = app
            .attach_session_secrets(open_req(
                saved.id,
                json!({"host": "10.1.1.1", "port": 5432, "database": "other"}),
            ))
            .unwrap();
        assert!(filled.password.is_none());
        assert_eq!(filled.plugin_config["host"], "10.1.1.1");
    }

    #[test]
    fn same_host_keeps_the_secret_and_the_client_database() {
        let app = svc();
        let saved = save_pg(&app, Some("s3cret"));
        let filled = app
            .attach_session_secrets(open_req(
                saved.id,
                json!({"host": "127.0.0.1", "port": 5432, "database": "other"}),
            ))
            .unwrap();
        assert_eq!(filled.password.as_deref(), Some("s3cret"));
        assert_eq!(filled.plugin_config["host"], "127.0.0.1");
        assert_eq!(filled.plugin_config["database"], "other");
    }

    #[test]
    fn mongo_connect_reinjects_userinfo_from_the_vault() {
        let app = svc();
        let saved = app
            .profiles()
            .save(SaveConnectionInput {
                id: None,
                name: "mongo".into(),
                engine: "documentdb".into(),
                color_id: "teal".into(),
                environment: "local".into(),
                read_only: true,
                notes: None,
                plugin_config: json!({
                    "uri": "mongodb://admin:s%40cret@127.0.0.1:27017/app?authSource=admin"
                }),
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
            })
            .unwrap();
        let mut req = open_req(
            saved.id,
            json!({"uri": "mongodb://127.0.0.1:27017/app?authSource=admin"}),
        );
        req.engine = "documentdb".into();
        let filled = app.attach_session_secrets(req).unwrap();
        assert_eq!(filled.password.as_deref(), Some("s@cret"));
        assert_eq!(filled.plugin_config["username"], "admin");
        assert_eq!(
            filled.plugin_config["uri"],
            "mongodb://127.0.0.1:27017/app?authSource=admin"
        );
    }

    #[test]
    fn attach_allows_passwordless_when_none_stored() {
        let app = svc();
        let saved = save_pg(&app, None);
        let filled = app
            .attach_session_secrets(open_req(saved.id, saved.plugin_config))
            .unwrap();
        assert!(filled.password.is_none());
    }
}
