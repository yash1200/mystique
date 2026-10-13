//! Application services: use-cases on top of storage + connectors.
//!
//! IPC / HTTP adapters call these — never repositories or drivers directly.

mod agent_chat;
mod agent_tools;
mod ai_secrets;
pub(crate) mod dbml;
mod history;
mod profiles;
mod query_library;
mod settings;
mod workspace_tabs;

pub use agent_chat::AgentChatService;
pub use agent_tools::{
    parse_allowlist, tool_specs, tool_specs_for, AgentColumnInfo, AgentConnectionInfo,
    AgentObjectInfo, AgentQueryDownload, AgentToolSpec, AgentTools, AgentWritePolicy, ToolCaller,
    AGENT_DEFAULT_CELL_CHARS, AGENT_MAX_CELL_CHARS, AGENT_MAX_ROWS, SETTING_MCP_ALLOWLIST,
    SETTING_MCP_POLICY,
};
pub use ai_secrets::AiSecretsService;
pub use history::HistoryService;
pub use profiles::ProfileService;
pub use query_library::QueryLibraryService;
pub use settings::SettingsService;
pub use workspace_tabs::WorkspaceTabsService;

use std::sync::Arc;

use crate::connectors::{ConnectionManager, OpenSessionRequest};
use crate::error::AppResult;
use crate::secrets::SecretStore;
use crate::storage::AppDatabase;

/// Bundle of application services.
///
/// Held in Tauri state (or tests) as `Arc<AppServices>`.
pub struct AppServices {
    pub db: Arc<AppDatabase>,
    pub connections: Arc<ConnectionManager>,
    pub secrets: Arc<SecretStore>,
}

impl AppServices {
    pub fn new(db: AppDatabase) -> Self {
        Self::with_secrets(db, ConnectionManager::bootstrap(), SecretStore::os())
    }

    pub fn with_connections(db: AppDatabase, connections: ConnectionManager) -> Self {
        Self::with_secrets(db, connections, SecretStore::os())
    }

    pub fn with_secrets(
        db: AppDatabase,
        connections: ConnectionManager,
        secrets: SecretStore,
    ) -> Self {
        let svc = Self {
            db: Arc::new(db),
            connections: Arc::new(connections),
            secrets: Arc::new(secrets),
        };
        let _ = svc.profiles().migrate_plaintext_secrets();
        svc
    }

    pub fn agent_tools(&self) -> AgentTools {
        AgentTools::new(
            Arc::clone(&self.db),
            Arc::clone(&self.connections),
            Arc::clone(&self.secrets),
        )
    }

    pub fn profiles(&self) -> ProfileService<'_> {
        ProfileService {
            db: &self.db,
            secrets: &self.secrets,
        }
    }

    /// Fill DB password + SSH tunnel from the profile / keychain / known_hosts.
    ///
    /// A saved profile's secret is only attached when the client is dialing
    /// that profile's host. Database name and SSL fields may still change.
    pub fn attach_session_secrets(&self, req: OpenSessionRequest) -> AppResult<OpenSessionRequest> {
        bind_open_request(&self.db, &self.secrets, req)
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

    /// Typed request password wins. A saved secret is attached only for that profile's host.
    pub fn password_for_request(&self, req: &OpenSessionRequest) -> AppResult<Option<String>> {
        Ok(bind_open_request(&self.db, &self.secrets, req.clone())?.password)
    }

    pub fn query_library(&self) -> QueryLibraryService<'_> {
        QueryLibraryService { db: &self.db }
    }

    pub fn workspace_tabs(&self) -> WorkspaceTabsService<'_> {
        WorkspaceTabsService { db: &self.db }
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
pub(crate) fn bind_open_request(
    db: &AppDatabase,
    secrets: &SecretStore,
    mut req: OpenSessionRequest,
) -> AppResult<OpenSessionRequest> {
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
        if req
            .environment
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
        {
            req.environment = Some(profile.environment.clone());
        }
        if req
            .connection_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
        {
            req.connection_name = Some(profile.name.clone());
        }
        req.profile_read_only = req.profile_read_only || profile.read_only;
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
    }

    lift_request_credentials(&mut req);
    let typed = nonempty(req.password.as_deref());
    if typed.is_none() && allow_stored {
        if let Some(id) = profile_id.as_deref() {
            let profiles = ProfileService { db, secrets };
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
                    "Stored password is missing from the OS keychain. Edit the connection and re-enter the password.".into(),
                ));
            }
        }
    }

    if allow_stored {
        // The test form sets use_saved_ssh false and omits ssh when the box
        // is unticked. Open and the agent leave the default, which fills it.
        if req.ssh.is_none() && req.use_saved_ssh {
            if let Some(profile) = &profile {
                let secret = profile_id.as_deref().and_then(|id| {
                    ProfileService { db, secrets }
                        .resolve_ssh_secret(id)
                        .ok()
                        .flatten()
                });
                req.ssh = crate::domain::SshTunnelRequest::from_config(&profile.ssh, secret);
            }
        } else if let Some(ssh) = req.ssh.as_mut() {
            let needs_secret = match ssh.auth {
                crate::domain::SshAuth::Password => nonempty(ssh.password.as_deref()).is_none(),
                crate::domain::SshAuth::Key => nonempty(ssh.passphrase.as_deref()).is_none(),
                crate::domain::SshAuth::Agent => false,
            };
            if needs_secret {
                if let Some(id) = profile_id.as_deref() {
                    if let Ok(Some(secret)) =
                        (ProfileService { db, secrets }).resolve_ssh_secret(id)
                    {
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
            if let Ok(Some(pw)) = (ProfileService { db, secrets }).resolve_tls_secret(id) {
                req.tls_key_passphrase = Some(pw);
            }
        }
    }
    Ok(req)
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
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
    let Some(pointer) = profiles
        .db
        .with_conn(|c| crate::storage::sqlite::repo::connections::get_secret_ref(c, id))?
    else {
        return Ok(());
    };
    if !crate::domain::is_keychain_pointer(&pointer)
        && !crate::domain::is_legacy_keychain_pointer(&pointer)
    {
        return Ok(());
    }
    let Some(user) = profiles
        .secrets
        .get_named(&crate::domain::user_secret_account(&pointer))?
    else {
        return Ok(());
    };
    if let Some(obj) = config.as_object_mut() {
        obj.insert("username".into(), serde_json::Value::String(user));
    }
    Ok(())
}

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
    fn attach_fills_password_from_keychain() {
        let app = svc();
        let saved = save_pg(&app, Some("s3cret"));
        let filled = app
            .attach_session_secrets(open_req(saved.id.clone(), saved.plugin_config.clone()))
            .unwrap();
        assert_eq!(filled.password.as_deref(), Some("s3cret"));
        assert!(filled.ssh.is_none());
    }

    #[test]
    fn attach_errors_when_stored_password_missing_from_keychain() {
        let app = svc();
        let saved = save_pg(&app, Some("s3cret"));
        let pointer = app
            .db
            .with_conn(|c| crate::storage::sqlite::repo::connections::get_secret_ref(c, &saved.id))
            .unwrap()
            .unwrap();
        app.secrets.delete_named(&pointer).unwrap();
        let err = app
            .attach_session_secrets(open_req(saved.id, saved.plugin_config))
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("keychain"),
            "expected keychain miss, got {msg}"
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
    fn mongo_connect_reinjects_userinfo_from_the_keychain() {
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
