//! MCP lifecycle: persist enable/token/port, start/stop the HTTP server.
//!
//! The token is sealed in the vault (`mcp_token`). At launch the server
//! starts without it; the first request loads it with keychain UI off, so a
//! locked vault fails that request with a clear error instead of prompting.

use std::sync::Arc;

use mystique_core::{AppError, AppResult, AppServices, Prompt, SecretKind};

use crate::clients::{self, Host, McpEndpoint, McpHarness};
use crate::server::{ensure_token, McpController, McpStatus, TokenGate};
use crate::settings::{DEFAULT_MCP_PORT, SETTING_MCP_ENABLED, SETTING_MCP_PORT};

/// Host-facing MCP service. Uses [`AppServices`] for tools + settings only.
#[derive(Clone)]
pub struct McpService {
    app: Arc<AppServices>,
    controller: McpController,
}

impl McpService {
    pub fn new(app: Arc<AppServices>) -> Self {
        Self {
            app,
            controller: McpController::new(),
        }
    }

    pub async fn status(&self) -> McpStatus {
        let enabled = self.enabled_setting();
        let mut status = self.controller.status(enabled).await;
        if status.url.is_none() {
            status.port = Some(self.port_setting());
            status.url = Some(self.configured_url());
        }
        if status.token.is_none() {
            status.token = self.token(Prompt::Allowed).await.ok();
        }
        status
    }

    pub fn enabled_setting(&self) -> bool {
        self.app
            .settings()
            .get(SETTING_MCP_ENABLED)
            .ok()
            .flatten()
            .map(|s| s.value == "true" || s.value == "1")
            .unwrap_or(false)
    }

    pub fn port_setting(&self) -> u16 {
        self.app
            .settings()
            .get(SETTING_MCP_PORT)
            .ok()
            .flatten()
            .and_then(|s| s.value.parse().ok())
            .filter(|p| *p > 0)
            .unwrap_or(DEFAULT_MCP_PORT)
    }

    async fn token(&self, prompt: Prompt) -> AppResult<String> {
        let app = Arc::clone(&self.app);
        tokio::task::spawn_blocking(move || load_token(&app, prompt))
            .await
            .map_err(|e| AppError::Internal(format!("MCP token: {e}")))?
    }

    /// From Settings: loading the token may show the one keychain prompt.
    pub async fn start(&self) -> AppResult<McpStatus> {
        let _ = self.app.settings().set(SETTING_MCP_ENABLED, "true");
        let token = self.token(Prompt::Allowed).await?;
        self.controller
            .start(
                self.app.agent_tools(),
                self.port_setting(),
                TokenGate::known(token),
            )
            .await
    }

    pub async fn stop(&self) -> AppResult<McpStatus> {
        let _ = self.app.settings().set(SETTING_MCP_ENABLED, "false");
        self.controller.stop().await;
        Ok(self.status().await)
    }

    pub async fn set_enabled(&self, enabled: bool) -> AppResult<McpStatus> {
        if enabled {
            self.start().await
        } else {
            self.stop().await
        }
    }

    pub async fn rotate_token(&self) -> AppResult<McpStatus> {
        let token = ensure_token(None);
        let app = Arc::clone(&self.app);
        let fresh = token.clone();
        tokio::task::spawn_blocking(move || app.vault().set(SecretKind::McpToken, &fresh))
            .await
            .map_err(|e| AppError::Internal(format!("MCP token: {e}")))??;
        if self.enabled_setting() {
            self.controller
                .start(
                    self.app.agent_tools(),
                    self.port_setting(),
                    TokenGate::known(token),
                )
                .await
        } else {
            Ok(self.status().await)
        }
    }

    /// At launch. Touches no keychain: the first request loads the token.
    pub async fn restore_if_enabled(&self) {
        if !self.enabled_setting() {
            return;
        }
        let app = Arc::clone(&self.app);
        let token =
            TokenGate::lazy(move || load_token(&app, Prompt::Never).map_err(|e| e.to_string()));
        let _ = self
            .controller
            .start(self.app.agent_tools(), self.port_setting(), token)
            .await;
    }

    pub async fn list_harnesses(&self) -> Vec<McpHarness> {
        let endpoint = self.endpoint().await.unwrap_or_else(|_| McpEndpoint {
            url: self.configured_url(),
            token: String::new(),
        });
        clients::list_available(&Host::current(), &endpoint)
    }

    pub async fn install_harness(&self, id: &str) -> AppResult<McpHarness> {
        let endpoint = self.endpoint().await?;
        clients::install(&Host::current(), id, &endpoint)
    }

    /// The live server's URL and token, for an in-app client (the external
    /// ACP agent). `None` while the server is stopped. May load the token
    /// from the vault (with keychain UI: the user just pressed Send).
    pub async fn running_endpoint(&self) -> AppResult<Option<McpEndpoint>> {
        let live = self.controller.status(self.enabled_setting()).await;
        if !live.running {
            return Ok(None);
        }
        let Some(url) = live.url else {
            return Ok(None);
        };
        let token = match live.token {
            Some(token) => token,
            None => self.token(Prompt::Allowed).await?,
        };
        Ok(Some(McpEndpoint { url, token }))
    }

    async fn endpoint(&self) -> AppResult<McpEndpoint> {
        let live = self.controller.status(self.enabled_setting()).await;
        let token = match live.token {
            Some(token) => token,
            None => self.token(Prompt::Allowed).await?,
        };
        Ok(McpEndpoint {
            url: live.url.unwrap_or_else(|| self.configured_url()),
            token,
        })
    }

    fn configured_url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.port_setting())
    }
}

/// The token from the vault, created there on first use.
fn load_token(app: &AppServices, prompt: Prompt) -> AppResult<String> {
    let vault = match prompt {
        Prompt::Allowed => app.vault(),
        Prompt::Never => app.vault().without_prompt(),
    };
    if let Some(token) = vault.get(SecretKind::McpToken)? {
        if token.trim().len() >= 16 {
            return Ok(token);
        }
    }
    let token = ensure_token(None);
    vault.set(SecretKind::McpToken, &token)?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_core::{KeySource, VaultError, VaultKey};
    use std::sync::atomic::{AtomicBool, Ordering};

    /// A keychain item whose ACL needs a prompt until the user allows it.
    struct NeedsPrompt {
        key: VaultKey,
        allowed: AtomicBool,
    }

    impl KeySource for NeedsPrompt {
        fn load(&self, prompt: Prompt) -> Result<VaultKey, VaultError> {
            match prompt {
                Prompt::Never if !self.allowed.load(Ordering::SeqCst) => Err(VaultError::Locked),
                _ => Ok(self.key.clone()),
            }
        }

        fn name(&self) -> &'static str {
            "keychain"
        }
    }

    /// One app process over a database file.
    fn process(db: &std::path::Path, source: Arc<dyn KeySource>) -> Arc<AppServices> {
        Arc::new(AppServices::with_secrets(
            mystique_core::AppDatabase::open(db).unwrap(),
            mystique_core::ConnectionManager::bootstrap(),
            mystique_core::SecretStore::with_key_source(source),
        ))
    }

    #[tokio::test]
    async fn the_token_is_created_once_in_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        let app = process(
            &dir.path().join("m.db"),
            Arc::new(mystique_core::secrets::MemoryKeySource::new()),
        );
        let mcp = McpService::new(Arc::clone(&app));
        let first = mcp.status().await.token.unwrap();
        assert!(first.len() >= 16);
        assert_eq!(mcp.status().await.token.as_deref(), Some(first.as_str()));
        assert_eq!(
            app.vault().get(SecretKind::McpToken).unwrap().as_deref(),
            Some(first.as_str())
        );

        let rotated = mcp.rotate_token().await.unwrap().token.unwrap();
        assert_ne!(rotated, first);
        assert_eq!(
            app.vault().get(SecretKind::McpToken).unwrap().as_deref(),
            Some(rotated.as_str())
        );
    }

    #[tokio::test]
    async fn a_locked_vault_fails_mcp_with_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("m.db");
        let source = Arc::new(NeedsPrompt {
            key: VaultKey::random(),
            allowed: AtomicBool::new(false),
        });
        // Saved earlier, while the user was there to answer the prompt.
        process(&db, source.clone())
            .vault()
            .set(SecretKind::McpToken, "0123456789abcdef0123")
            .unwrap();

        // Next launch: the server is restored and a request arrives first.
        let app = process(&db, source.clone());
        let gate = {
            let app = Arc::clone(&app);
            TokenGate::lazy(move || load_token(&app, Prompt::Never).map_err(|e| e.to_string()))
        };
        let err = gate.get().await.unwrap_err();
        assert!(err.contains("Saved passwords are locked"), "{err}");
        assert!(err.contains("Retry keychain"), "{err}");
        assert_eq!(app.vault().status().state, "idle", "not cached as a denial");

        // After "Always Allow", the next request gets the token without UI.
        source.allowed.store(true, Ordering::SeqCst);
        assert_eq!(gate.get().await.unwrap(), "0123456789abcdef0123");
    }
}
