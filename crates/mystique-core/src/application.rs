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
    tool_specs, AgentColumnInfo, AgentConnectionInfo, AgentObjectInfo, AgentQueryDownload,
    AgentToolSpec, AgentTools,
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
    pub fn attach_session_secrets(
        &self,
        mut req: OpenSessionRequest,
    ) -> AppResult<OpenSessionRequest> {
        if let Some(id) = req
            .profile_id
            .as_deref()
            .or(req.session_id.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Ok(profile) = self.profiles().get(id) {
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
            }
        }
        req.password = self.password_for_request(&req)?;
        if req
            .password
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
        {
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
        if req.ssh.is_none() {
            if let Some(id) = req
                .profile_id
                .as_deref()
                .or(req.session_id.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if let Ok(profile) = self.profiles().get(id) {
                    let secret = self.profiles().resolve_ssh_secret(id).ok().flatten();
                    req.ssh = crate::domain::SshTunnelRequest::from_config(&profile.ssh, secret);
                }
            }
        } else if let Some(ssh) = req.ssh.as_mut() {
            let needs_secret = match ssh.auth {
                crate::domain::SshAuth::Password => ssh
                    .password
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .is_none(),
                crate::domain::SshAuth::Key => ssh
                    .passphrase
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .is_none(),
                crate::domain::SshAuth::Agent => false,
            };
            if needs_secret {
                if let Some(id) = req
                    .profile_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    if let Ok(Some(secret)) = self.profiles().resolve_ssh_secret(id) {
                        match ssh.auth {
                            crate::domain::SshAuth::Password => ssh.password = Some(secret),
                            crate::domain::SshAuth::Key => ssh.passphrase = Some(secret),
                            crate::domain::SshAuth::Agent => {}
                        }
                    }
                }
            }
        }
        if let Some(ssh) = req.ssh.as_mut() {
            if ssh.trusted_fingerprint.is_none() {
                if let Ok(Some(known)) = self.lookup_ssh_host_key(&ssh.host, ssh.port) {
                    ssh.trusted_fingerprint = Some(known.fingerprint);
                }
            }
        }
        if req
            .password
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
        {
            if let Some(id) = req
                .profile_id
                .as_deref()
                .or(req.session_id.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if let Ok(profile) = self.profiles().get(id) {
                    if profile.password_set {
                        return Err(crate::error::AppError::SecretStore(
                            "Stored password is missing from the OS keychain. Edit the connection and re-enter the password.".into(),
                        ));
                    }
                }
            }
        }
        if req
            .tls_key_passphrase
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
        {
            if let Some(id) = req
                .profile_id
                .as_deref()
                .or(req.session_id.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if let Ok(Some(pw)) = self.profiles().resolve_tls_secret(id) {
                    req.tls_key_passphrase = Some(pw);
                }
            }
        }
        Ok(req)
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

    /// Typed request password wins; otherwise look up the profile in the keychain.
    pub fn password_for_request(&self, req: &OpenSessionRequest) -> AppResult<Option<String>> {
        let typed = req
            .password
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(pw) = typed {
            return Ok(Some(pw.to_string()));
        }
        let id = req
            .profile_id
            .as_deref()
            .or(req.session_id.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        match id {
            Some(id) => self.profiles().resolve_password(id),
            None => Ok(None),
        }
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
        }
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
        app.secrets.delete(&saved.id).unwrap();
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
    fn attach_allows_passwordless_when_none_stored() {
        let app = svc();
        let saved = save_pg(&app, None);
        let filled = app
            .attach_session_secrets(open_req(saved.id, saved.plugin_config))
            .unwrap();
        assert!(filled.password.is_none());
    }
}
