//! Settings → AI → Agent: built-in (default) or an external ACP agent.
//!
//! The command, its arguments and plain environment values live in the
//! settings table. An environment value whose name looks secret (`…KEY`,
//! `…TOKEN`, `…SECRET`, `…PASSWORD`, …) is sealed in the vault as
//! `ai:acp-env:<NAME>` and never sent back to the window.

use std::path::{Path, PathBuf};

use mystique_core::{AppError, AppResult, AppServices, SecretKind};
use serde::{Deserialize, Serialize};

pub const SETTING_AGENT_KIND: &str = "ai.agent.kind";
pub const SETTING_ACP_COMMAND: &str = "ai.acp.command";
pub const SETTING_ACP_ARGS: &str = "ai.acp.args";
pub const SETTING_ACP_ENV: &str = "ai.acp.env";
/// The agent's name from its last `initialize`, for the pane header.
pub const SETTING_ACP_AGENT_NAME: &str = "ai.acp.agent_name";

pub const KIND_BUILTIN: &str = "builtin";
pub const KIND_ACP: &str = "acp";

/// The provider tag an external agent's chats carry.
pub const ACP_PROVIDER: &str = "acp";

/// Where an app started from Finder may not look: added to the agent's PATH.
const EXTRA_PATH: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

/// One environment variable, as the window sees it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct AcpEnvVar {
    pub name: String,
    /// Plain values only. A secret's value never leaves the vault.
    #[serde(default)]
    pub value: Option<String>,
    pub secret: bool,
    /// A secret is saved in the vault.
    pub saved: bool,
}

/// Settings → AI → Agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AcpSettingsView {
    /// `builtin` or `acp`.
    pub kind: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<AcpEnvVar>,
    /// What the pane header calls the external agent.
    pub agent_label: String,
}

/// One environment variable to save. An empty value of a secret that is
/// already saved keeps the saved one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct AcpEnvInput {
    pub name: String,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SaveAcpSettings {
    pub kind: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<AcpEnvInput>,
}

/// How a variable is kept in the settings table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StoredEnv {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(default)]
    secret: bool,
}

/// True for names that usually hold a credential.
pub fn looks_secret(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "KEY",
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "CREDENTIAL",
        "AUTH",
    ]
    .iter()
    .any(|word| upper.contains(word))
}

fn valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn vault_name(name: &str) -> String {
    format!("acp-env:{name}")
}

fn get(app: &AppServices, key: &str) -> Option<String> {
    app.settings().get(key).ok().flatten().map(|s| s.value)
}

fn stored_env(app: &AppServices) -> Vec<StoredEnv> {
    get(app, SETTING_ACP_ENV)
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn stored_args(app: &AppServices) -> Vec<String> {
    get(app, SETTING_ACP_ARGS)
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn kind(app: &AppServices) -> String {
    match get(app, SETTING_AGENT_KIND).as_deref().map(str::trim) {
        Some(KIND_ACP) => KIND_ACP.into(),
        _ => KIND_BUILTIN.into(),
    }
}

/// The command's file name, e.g. `claude-agent-acp` or `npx`.
fn command_label(command: &str, args: &[String]) -> String {
    let base = Path::new(command.trim())
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    // `npx -y @scope/pkg` → the package says more than `npx`.
    if matches!(base.as_str(), "npx" | "bunx" | "pnpx" | "uvx") {
        if let Some(pkg) = args.iter().find(|a| !a.starts_with('-')) {
            return pkg.rsplit('/').next().unwrap_or(pkg).to_string();
        }
    }
    base
}

pub fn agent_label(app: &AppServices) -> String {
    if let Some(name) = get(app, SETTING_ACP_AGENT_NAME).filter(|s| !s.trim().is_empty()) {
        return name;
    }
    let command = get(app, SETTING_ACP_COMMAND).unwrap_or_default();
    let label = command_label(&command, &stored_args(app));
    if label.is_empty() {
        "External agent".into()
    } else {
        label
    }
}

/// Never loads the vault key.
pub fn view(app: &AppServices) -> AcpSettingsView {
    let env = stored_env(app)
        .into_iter()
        .map(|e| {
            let saved = e.secret
                && app
                    .vault()
                    .has(SecretKind::Ai(&vault_name(&e.name)))
                    .unwrap_or(false);
            AcpEnvVar {
                value: if e.secret { None } else { e.value },
                name: e.name,
                secret: e.secret,
                saved,
            }
        })
        .collect();
    AcpSettingsView {
        kind: kind(app),
        command: get(app, SETTING_ACP_COMMAND).unwrap_or_default(),
        args: stored_args(app),
        env,
        agent_label: agent_label(app),
    }
}

pub fn save(app: &AppServices, input: SaveAcpSettings) -> AppResult<AcpSettingsView> {
    let kind = match input.kind.trim() {
        KIND_ACP => KIND_ACP,
        KIND_BUILTIN | "" => KIND_BUILTIN,
        other => return Err(AppError::Invalid(format!("unknown agent kind `{other}`"))),
    };
    let command = input.command.trim().to_string();
    if kind == KIND_ACP && command.is_empty() {
        return Err(AppError::Invalid(
            "Enter the command that starts the agent.".into(),
        ));
    }
    let before = stored_env(app);
    let mut stored = Vec::new();
    for var in input.env {
        let name = var.name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        if !valid_env_name(&name) {
            return Err(AppError::Invalid(format!(
                "`{name}` is not a valid environment variable name."
            )));
        }
        if stored.iter().any(|s: &StoredEnv| s.name == name) {
            return Err(AppError::Invalid(format!("`{name}` is listed twice.")));
        }
        let value = var.value.unwrap_or_default();
        let was_secret = before.iter().any(|b| b.name == name && b.secret);
        let secret = looks_secret(&name) || was_secret && value.is_empty();
        if secret {
            if !value.trim().is_empty() {
                app.vault()
                    .set(SecretKind::Ai(&vault_name(&name)), value.trim())?;
            }
            stored.push(StoredEnv {
                name,
                value: None,
                secret: true,
            });
        } else {
            if was_secret {
                app.vault().delete(SecretKind::Ai(&vault_name(&name)))?;
            }
            stored.push(StoredEnv {
                name,
                value: Some(value),
                secret: false,
            });
        }
    }
    for old in before.iter().filter(|b| b.secret) {
        if !stored.iter().any(|s| s.name == old.name) {
            app.vault().delete(SecretKind::Ai(&vault_name(&old.name)))?;
        }
    }
    let args: Vec<String> = input.args.into_iter().filter(|a| !a.is_empty()).collect();
    let previous_command = get(app, SETTING_ACP_COMMAND).unwrap_or_default();
    let settings = app.settings();
    settings.set(SETTING_AGENT_KIND, kind)?;
    settings.set(SETTING_ACP_COMMAND, &command)?;
    settings.set(SETTING_ACP_ARGS, &serde_json::to_string(&args)?)?;
    settings.set(SETTING_ACP_ENV, &serde_json::to_string(&stored)?)?;
    if previous_command != command || stored_args(app) != args {
        let _ = settings.delete(SETTING_ACP_AGENT_NAME);
    }
    Ok(view(app))
}

/// Remember the agent's own name for the header.
pub fn remember_agent_name(app: &AppServices, name: &str) {
    let name = name.trim();
    if !name.is_empty() {
        let _ = app.settings().set(SETTING_ACP_AGENT_NAME, name);
    }
}

/// Everything needed to start the agent. Loads secret values from the vault.
pub(crate) struct AgentConfig {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Secret values, for masking.
    pub secrets: Vec<String>,
}

pub(crate) fn load(app: &AppServices) -> AppResult<AgentConfig> {
    let program = get(app, SETTING_ACP_COMMAND).unwrap_or_default();
    if program.trim().is_empty() {
        return Err(AppError::Invalid(
            "No external agent is set up. Add its command in Settings → AI.".into(),
        ));
    }
    let mut env = Vec::new();
    let mut secrets = Vec::new();
    for var in stored_env(app) {
        if var.secret {
            if let Some(value) = app.vault().get(SecretKind::Ai(&vault_name(&var.name)))? {
                secrets.push(value.clone());
                env.push((var.name, value));
            }
        } else {
            env.push((var.name, var.value.unwrap_or_default()));
        }
    }
    let path = env
        .iter()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| v.clone())
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    let path = with_extra_path(&path);
    env.retain(|(k, _)| k != "PATH");
    env.push(("PATH".into(), path.clone()));
    let program = resolve_program(program.trim(), &path).ok_or_else(|| {
        AppError::Invalid(format!(
            "Could not find `{}`. Use its full path (apps started from Finder do not see your shell's PATH), or add PATH under Environment.",
            program.trim()
        ))
    })?;
    Ok(AgentConfig {
        program: program.to_string_lossy().to_string(),
        args: stored_args(app),
        env,
        secrets,
    })
}

fn with_extra_path(path: &str) -> String {
    let mut parts: Vec<String> = std::env::split_paths(path)
        .map(|p| p.to_string_lossy().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    for extra in EXTRA_PATH {
        if !parts.iter().any(|p| p == extra) {
            parts.push((*extra).to_string());
        }
    }
    std::env::join_paths(parts)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string())
}

/// A bare command is looked up on `path`; a path must exist.
fn resolve_program(program: &str, path: &str) -> Option<PathBuf> {
    let given = Path::new(program);
    if given.components().count() > 1 || given.is_absolute() {
        return given.is_file().then(|| given.to_path_buf());
    }
    std::env::split_paths(path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn app() -> Arc<AppServices> {
        Arc::new(AppServices::with_secrets(
            mystique_core::AppDatabase::open_in_memory().unwrap(),
            mystique_core::ConnectionManager::bootstrap(),
            mystique_core::SecretStore::memory(),
        ))
    }

    fn save_input(env: Vec<(&str, &str)>) -> SaveAcpSettings {
        SaveAcpSettings {
            kind: KIND_ACP.into(),
            command: "npx".into(),
            args: vec!["-y".into(), "@agentclientprotocol/claude-agent-acp".into()],
            env: env
                .into_iter()
                .map(|(n, v)| AcpEnvInput {
                    name: n.into(),
                    value: Some(v.into()),
                })
                .collect(),
        }
    }

    #[test]
    fn secret_looking_values_go_to_the_vault_and_never_back_to_the_window() {
        let app = app();
        let view = save(
            &app,
            save_input(vec![("ANTHROPIC_API_KEY", "sk-ant-123456"), ("DEBUG", "1")]),
        )
        .unwrap();
        assert_eq!(view.kind, KIND_ACP);
        assert_eq!(view.agent_label, "claude-agent-acp");
        let key = view
            .env
            .iter()
            .find(|e| e.name == "ANTHROPIC_API_KEY")
            .unwrap();
        assert!(key.secret && key.saved && key.value.is_none());
        let debug = view.env.iter().find(|e| e.name == "DEBUG").unwrap();
        assert_eq!(debug.value.as_deref(), Some("1"));
        let raw = get(&app, SETTING_ACP_ENV).unwrap();
        assert!(!raw.contains("sk-ant-123456"), "{raw}");
        assert_eq!(
            app.vault()
                .get(SecretKind::Ai("acp-env:ANTHROPIC_API_KEY"))
                .unwrap()
                .as_deref(),
            Some("sk-ant-123456")
        );

        // Saving again with an empty value keeps the saved secret.
        save(
            &app,
            save_input(vec![("ANTHROPIC_API_KEY", ""), ("DEBUG", "1")]),
        )
        .unwrap();
        assert!(app
            .vault()
            .has(SecretKind::Ai("acp-env:ANTHROPIC_API_KEY"))
            .unwrap());
        // Removing the variable removes the secret.
        save(&app, save_input(vec![("DEBUG", "1")])).unwrap();
        assert!(!app
            .vault()
            .has(SecretKind::Ai("acp-env:ANTHROPIC_API_KEY"))
            .unwrap());
    }

    #[test]
    fn bad_input_is_refused_and_builtin_is_the_default() {
        let app = app();
        assert_eq!(view(&app).kind, KIND_BUILTIN);
        assert!(save(&app, save_input(vec![("1BAD", "x")])).is_err());
        assert!(save(&app, save_input(vec![("A", "x"), ("A", "y")])).is_err());
        let mut no_command = save_input(vec![]);
        no_command.command = " ".into();
        assert!(save(&app, no_command).is_err());
    }

    #[test]
    fn a_missing_command_says_how_to_fix_it() {
        let app = app();
        let mut input = save_input(vec![]);
        input.command = "definitely-not-a-real-agent-binary".into();
        save(&app, input).unwrap();
        let err = load(&app).err().unwrap().to_string();
        assert!(err.contains("full path"), "{err}");
    }

    #[test]
    fn labels_and_secret_names() {
        assert_eq!(
            command_label("/usr/local/bin/claude-agent-acp", &[]),
            "claude-agent-acp"
        );
        assert_eq!(
            command_label(
                "npx",
                &["-y".into(), "@zed-industries/claude-code-acp".into()]
            ),
            "claude-code-acp"
        );
        assert!(looks_secret("ANTHROPIC_API_KEY"));
        assert!(looks_secret("GITHUB_TOKEN"));
        assert!(!looks_secret("DEBUG"));
    }
}
