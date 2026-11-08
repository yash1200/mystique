//! SSH tunnel on the connection profile (transport), not the plugin.

use serde::{Deserialize, Serialize};

/// How we authenticate to the bastion. One hop; no SOCKS/HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum SshAuth {
    #[default]
    Password,
    Key,
    Agent,
}

/// Persisted SSH fields. Secrets never live here — see `ssh_secret_ref`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub enabled: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub host: String,
    #[serde(default = "default_ssh_port")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub port: u16,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub user: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub auth: SshAuth,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub private_key_path: String,
}

fn default_ssh_port() -> u16 {
    22
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: String::new(),
            port: 22,
            user: String::new(),
            auth: SshAuth::Password,
            private_key_path: String::new(),
        }
    }
}

impl SshConfig {
    pub fn is_active(&self) -> bool {
        self.enabled && !self.host.trim().is_empty()
    }

    pub fn normalized_port(&self) -> u16 {
        if self.port == 0 {
            22
        } else {
            self.port
        }
    }
}

/// Runtime tunnel params on `OpenSessionRequest` (includes secrets).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct SshTunnelRequest {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: SshAuth,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub private_key_path: Option<String>,
    #[serde(default)]
    pub passphrase: Option<String>,
    /// SHA256 fingerprint already trusted (known_hosts or this-session accept).
    #[serde(default)]
    pub trusted_fingerprint: Option<String>,
}

impl SshTunnelRequest {
    pub fn from_config(cfg: &SshConfig, secret: Option<String>) -> Option<Self> {
        if !cfg.is_active() {
            return None;
        }
        let secret = secret.filter(|s| !s.trim().is_empty());
        Some(Self {
            host: cfg.host.trim().to_string(),
            port: cfg.normalized_port(),
            user: cfg.user.trim().to_string(),
            auth: cfg.auth,
            password: if cfg.auth == SshAuth::Password {
                secret.clone()
            } else {
                None
            },
            private_key_path: {
                let p = cfg.private_key_path.trim();
                if p.is_empty() {
                    None
                } else {
                    Some(p.to_string())
                }
            },
            passphrase: if cfg.auth == SshAuth::Key {
                secret
            } else {
                None
            },
            trusted_fingerprint: None,
        })
    }
}

/// Keychain account for the bastion password or key passphrase.
pub fn ssh_secret_account(connection_id: &str) -> String {
    format!("conn.{connection_id}.ssh")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_when_disabled_or_blank_host() {
        let mut cfg = SshConfig {
            enabled: true,
            host: "  ".into(),
            ..SshConfig::default()
        };
        assert!(!cfg.is_active());
        cfg.host = "bastion".into();
        assert!(cfg.is_active());
        cfg.enabled = false;
        assert!(!cfg.is_active());
    }

    #[test]
    fn from_config_puts_secret_on_password_auth() {
        let cfg = SshConfig {
            enabled: true,
            host: "b".into(),
            user: "u".into(),
            auth: SshAuth::Password,
            ..SshConfig::default()
        };
        let req = SshTunnelRequest::from_config(&cfg, Some("pw".into())).unwrap();
        assert_eq!(req.password.as_deref(), Some("pw"));
        assert!(req.passphrase.is_none());
    }
}
