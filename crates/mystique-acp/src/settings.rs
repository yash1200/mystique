//! Settings → AI → Agent: the built-in agent (default) and the external ACP
//! agents the user added, one of them maybe the default.
//!
//! The list lives in the settings table as JSON (`ai.acp.agents`): per agent
//! its id, catalog key (or none for a custom agent), command, arguments and
//! plain environment values. An environment value whose name looks secret
//! (`…KEY`, `…TOKEN`, `…SECRET`, `…PASSWORD`, …) is sealed in the vault as
//! `ai:acp-env:<agent id>:<NAME>` and never sent back to the window.
//! Removing an agent removes its secrets.
//!
//! Before the list there was one agent (`ai.acp.command`, …). It is moved
//! into the list once, as the agent `custom`, whose secrets keep their old
//! vault names (`ai:acp-env:<NAME>`), so the move never opens the vault.

use std::path::{Path, PathBuf};

use mystique_core::{AppError, AppResult, AppServices, SecretKind};
use serde::{Deserialize, Serialize};

use crate::catalog;
use crate::detect;

/// The added agents (JSON list of [`StoredAgent`]).
pub const SETTING_AGENTS: &str = "ai.acp.agents";
/// `builtin` or the id of an added agent: what a new chat uses.
pub const SETTING_DEFAULT_AGENT: &str = "ai.agent.default";
/// The built-in agent's id in the agent menu.
pub const BUILTIN: &str = "builtin";

/// The provider tag an external agent's chats carry. The chat's `model` is
/// the agent's id, so each chat remembers which agent it talks to.
pub const ACP_PROVIDER: &str = "acp";

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

/// One added agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct AcpAgentView {
    pub id: String,
    /// The catalog entry it was added from; `None` for a custom agent.
    #[serde(default)]
    pub catalog_id: Option<String>,
    /// What the agent menu and the pane header call it.
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<AcpEnvVar>,
}

/// Settings → AI → Agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AcpSettingsView {
    /// `builtin` or an agent id.
    pub default_agent: String,
    pub agents: Vec<AcpAgentView>,
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

/// Add or change one agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct SaveAcpAgent {
    /// The agent to change; `None` adds one.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub catalog_id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<AcpEnvInput>,
    /// Also make it the default for new chats.
    #[serde(default)]
    pub make_default: bool,
}

/// The saved agent's id and the settings after the save.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AcpAgentSaved {
    pub id: String,
    pub settings: AcpSettingsView,
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

/// How an agent is kept in the settings table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoredAgent {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    catalog_id: Option<String>,
    label: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: Vec<StoredEnv>,
    /// The agent's own name from its last `initialize`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    agent_name: Option<String>,
}

impl StoredAgent {
    fn display(&self) -> String {
        self.agent_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.label)
            .to_string()
    }
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

fn vault_name(agent_id: &str, name: &str) -> String {
    format!("acp-env:{agent_id}:{name}")
}

fn get(app: &AppServices, key: &str) -> Option<String> {
    app.settings().get(key).ok().flatten().map(|s| s.value)
}

fn stored(app: &AppServices) -> Vec<StoredAgent> {
    get(app, SETTING_AGENTS)
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn store(app: &AppServices, agents: &[StoredAgent]) -> AppResult<()> {
    app.settings()
        .set(SETTING_AGENTS, &serde_json::to_string(agents)?)?;
    Ok(())
}

/// `builtin` or an agent id that exists.
pub fn default_agent(app: &AppServices) -> String {
    let agents = stored(app);
    match get(app, SETTING_DEFAULT_AGENT) {
        Some(id) if agents.iter().any(|a| a.id == id) => id,
        _ => BUILTIN.into(),
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
    if base.is_empty() {
        "External agent".into()
    } else {
        base
    }
}

/// Never loads the vault key.
pub fn view(app: &AppServices) -> AcpSettingsView {
    let agents = stored(app)
        .into_iter()
        .map(|agent| {
            let env = agent
                .env
                .iter()
                .map(|e| {
                    let saved = e.secret
                        && app
                            .vault()
                            .has(SecretKind::Ai(&vault_name(&agent.id, &e.name)))
                            .unwrap_or(false);
                    AcpEnvVar {
                        value: if e.secret { None } else { e.value.clone() },
                        name: e.name.clone(),
                        secret: e.secret,
                        saved,
                    }
                })
                .collect();
            AcpAgentView {
                label: agent.display(),
                id: agent.id,
                catalog_id: agent.catalog_id,
                command: agent.command,
                args: agent.args,
                env,
            }
        })
        .collect();
    AcpSettingsView {
        default_agent: default_agent(app),
        agents,
    }
}

/// `custom-1`, `custom-2`, … — the first one not taken.
fn next_custom_id(agents: &[StoredAgent]) -> String {
    (1..)
        .map(|n| format!("custom-{n}"))
        .find(|id| !agents.iter().any(|a| &a.id == id))
        .unwrap_or_else(|| "custom-x".into())
}

/// Validate `input` and seal its secrets for agent `id`. Returns what to
/// store; deletes the vault entries of secrets the input drops.
fn store_env(
    app: &AppServices,
    id: &str,
    before: &[StoredEnv],
    input: Vec<AcpEnvInput>,
) -> AppResult<Vec<StoredEnv>> {
    let mut stored = Vec::new();
    for var in input {
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
                    .set(SecretKind::Ai(&vault_name(id, &name)), value.trim())?;
            }
            stored.push(StoredEnv {
                name,
                value: None,
                secret: true,
            });
        } else {
            if was_secret {
                app.vault().delete(SecretKind::Ai(&vault_name(id, &name)))?;
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
            app.vault()
                .delete(SecretKind::Ai(&vault_name(id, &old.name)))?;
        }
    }
    Ok(stored)
}

/// Add an agent, or change one. Secret-looking values go to the vault (may
/// load the vault key).
pub fn save_agent(app: &AppServices, input: SaveAcpAgent) -> AppResult<AcpAgentSaved> {
    let command = input.command.trim().to_string();
    if command.is_empty() {
        return Err(AppError::Invalid(
            "Enter the command that starts the agent.".into(),
        ));
    }
    let catalog_id = input
        .catalog_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let entry = match catalog_id.as_deref() {
        Some(key) => Some(
            catalog::find(key)
                .ok_or_else(|| AppError::Invalid(format!("Unknown agent `{key}`.")))?,
        ),
        None => None,
    };
    let mut agents = stored(app);
    let requested = input
        .id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != BUILTIN);
    let position = match requested {
        Some(id) => Some(
            agents
                .iter()
                .position(|a| a.id == id)
                .ok_or_else(|| AppError::Invalid("That agent was removed.".into()))?,
        ),
        // A catalog agent is added once: adding it again updates it.
        None => entry.and_then(|e| agents.iter().position(|a| a.id == e.id)),
    };
    let id = match position.and_then(|i| agents.get(i)) {
        Some(existing) => existing.id.clone(),
        None => match entry {
            Some(e) => e.id.to_string(),
            None => next_custom_id(&agents),
        },
    };
    let before = position
        .and_then(|i| agents.get(i))
        .map(|a| a.env.clone())
        .unwrap_or_default();
    let env = store_env(app, &id, &before, input.env)?;
    let args: Vec<String> = input.args.into_iter().filter(|a| !a.is_empty()).collect();
    let label = input
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| entry.map(|e| e.name.to_string()))
        .unwrap_or_else(|| command_label(&command, &args));
    let previous = position.and_then(|i| agents.get(i)).cloned();
    let agent_name = previous
        .as_ref()
        .filter(|p| p.command == command && p.args == args)
        .and_then(|p| p.agent_name.clone());
    let agent = StoredAgent {
        id: id.clone(),
        catalog_id: catalog_id.or_else(|| previous.and_then(|p| p.catalog_id)),
        label,
        command,
        args,
        env,
        agent_name,
    };
    match position.and_then(|i| agents.get_mut(i)) {
        Some(slot) => *slot = agent,
        None => agents.push(agent),
    }
    store(app, &agents)?;
    if input.make_default {
        app.settings().set(SETTING_DEFAULT_AGENT, &id)?;
    }
    Ok(AcpAgentSaved {
        id,
        settings: view(app),
    })
}

/// Remove an agent and its vault secrets. If it was the default, the
/// built-in agent is the default again.
pub fn remove_agent(app: &AppServices, id: &str) -> AppResult<AcpSettingsView> {
    let mut agents = stored(app);
    let position = agents
        .iter()
        .position(|a| a.id == id)
        .ok_or_else(|| AppError::Invalid("That agent was already removed.".into()))?;
    let agent = agents.remove(position);
    for var in agent.env.iter().filter(|e| e.secret) {
        app.vault()
            .delete(SecretKind::Ai(&vault_name(&agent.id, &var.name)))?;
    }
    store(app, &agents)?;
    if get(app, SETTING_DEFAULT_AGENT).as_deref() == Some(id) {
        app.settings().set(SETTING_DEFAULT_AGENT, BUILTIN)?;
    }
    Ok(view(app))
}

/// What new chats use: `builtin` or an agent id.
pub fn set_default(app: &AppServices, id: &str) -> AppResult<AcpSettingsView> {
    let id = id.trim();
    if id != BUILTIN && !stored(app).iter().any(|a| a.id == id) {
        return Err(AppError::Invalid("That agent was removed.".into()));
    }
    app.settings().set(SETTING_DEFAULT_AGENT, id)?;
    Ok(view(app))
}

/// Remember the agent's own name for the menu and header.
pub fn remember_agent_name(app: &AppServices, id: &str, name: &str) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let mut agents = stored(app);
    if let Some(agent) = agents.iter_mut().find(|a| a.id == id) {
        if agent.agent_name.as_deref() != Some(name) {
            agent.agent_name = Some(name.to_string());
            let _ = store(app, &agents);
        }
    }
}

/// The agent a turn or a test uses: `requested`, or the default when empty.
fn pick(app: &AppServices, requested: &str) -> AppResult<StoredAgent> {
    let agents = stored(app);
    let requested = requested.trim();
    let id = if requested.is_empty() {
        default_agent(app)
    } else {
        requested.to_string()
    };
    if id == BUILTIN {
        return Err(AppError::Invalid(
            "No external agent is picked. Add one in Settings → AI → Agent.".into(),
        ));
    }
    agents.into_iter().find(|a| a.id == id).ok_or_else(|| {
        AppError::Invalid("That agent was removed. Pick another one in the agent menu.".into())
    })
}

/// What the pane header calls agent `id`.
#[cfg(test)]
fn agent_label(app: &AppServices, id: &str) -> String {
    pick(app, id)
        .map(|a| a.display())
        .unwrap_or_else(|_| "External agent".into())
}

/// Everything needed to start an agent. Loads secret values from the vault.
pub(crate) struct AgentConfig {
    pub id: String,
    pub catalog_id: Option<String>,
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Secret values, for masking.
    pub secrets: Vec<String>,
}

pub(crate) fn load(app: &AppServices, requested: &str) -> AppResult<AgentConfig> {
    let agent = pick(app, requested)?;
    let mut env = Vec::new();
    let mut secrets = Vec::new();
    for var in &agent.env {
        if var.secret {
            if let Some(value) = app
                .vault()
                .get(SecretKind::Ai(&vault_name(&agent.id, &var.name)))?
            {
                secrets.push(value.clone());
                env.push((var.name.clone(), value));
            }
        } else {
            env.push((var.name.clone(), var.value.clone().unwrap_or_default()));
        }
    }
    let base = env
        .iter()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| v.clone())
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    let program = agent.command.trim();
    let given = Path::new(program);
    let path = detect::launch_path(&base, given.parent().filter(|_| given.is_absolute()));
    env.retain(|(k, _)| k != "PATH");
    env.push(("PATH".into(), path.clone()));
    let resolved = resolve_program(program, &path).ok_or_else(|| {
        AppError::Invalid(format!(
            "Could not find `{program}`. Rescan in Settings → AI → Agent, use its full path, or add PATH under Environment."
        ))
    })?;
    Ok(AgentConfig {
        id: agent.id,
        catalog_id: agent.catalog_id,
        program: resolved.to_string_lossy().to_string(),
        args: agent.args,
        env,
        secrets,
    })
}

/// A bare command is looked up on `path`; a path must exist.
fn resolve_program(program: &str, path: &str) -> Option<PathBuf> {
    let given = Path::new(program);
    if given.components().count() > 1 || given.is_absolute() {
        return given.is_file().then(|| given.to_path_buf());
    }
    detect::find_program(program, path)
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

    fn input(id: Option<&str>, env: Vec<(&str, &str)>) -> SaveAcpAgent {
        SaveAcpAgent {
            id: id.map(str::to_string),
            catalog_id: None,
            label: None,
            command: "npx".into(),
            args: vec!["-y".into(), "@agentclientprotocol/claude-agent-acp".into()],
            env: env
                .into_iter()
                .map(|(n, v)| AcpEnvInput {
                    name: n.into(),
                    value: Some(v.into()),
                })
                .collect(),
            make_default: false,
        }
    }

    fn has_secret(app: &AppServices, name: &str) -> bool {
        app.vault().has(SecretKind::Ai(name)).unwrap()
    }

    #[test]
    fn secret_looking_values_go_to_the_vault_and_never_back_to_the_window() {
        let app = app();
        let saved = save_agent(
            &app,
            input(
                None,
                vec![("ANTHROPIC_API_KEY", "sk-ant-123456"), ("DEBUG", "1")],
            ),
        )
        .unwrap();
        assert_eq!(saved.id, "custom-1");
        let agent = &saved.settings.agents[0];
        assert_eq!(agent.label, "claude-agent-acp");
        let key = agent
            .env
            .iter()
            .find(|e| e.name == "ANTHROPIC_API_KEY")
            .unwrap();
        assert!(key.secret && key.saved && key.value.is_none());
        let debug = agent.env.iter().find(|e| e.name == "DEBUG").unwrap();
        assert_eq!(debug.value.as_deref(), Some("1"));
        let raw = get(&app, SETTING_AGENTS).unwrap();
        assert!(!raw.contains("sk-ant-123456"), "{raw}");
        assert_eq!(
            app.vault()
                .get(SecretKind::Ai("acp-env:custom-1:ANTHROPIC_API_KEY"))
                .unwrap()
                .as_deref(),
            Some("sk-ant-123456")
        );

        // Saving again with an empty value keeps the saved secret.
        save_agent(
            &app,
            input(
                Some("custom-1"),
                vec![("ANTHROPIC_API_KEY", ""), ("DEBUG", "1")],
            ),
        )
        .unwrap();
        assert!(has_secret(&app, "acp-env:custom-1:ANTHROPIC_API_KEY"));
        // Removing the variable removes the secret.
        save_agent(&app, input(Some("custom-1"), vec![("DEBUG", "1")])).unwrap();
        assert!(!has_secret(&app, "acp-env:custom-1:ANTHROPIC_API_KEY"));
    }

    #[test]
    fn agents_are_added_listed_changed_and_removed_with_their_secrets() {
        let app = app();
        assert_eq!(
            view(&app),
            AcpSettingsView {
                default_agent: BUILTIN.into(),
                agents: vec![],
            }
        );
        let mut claude = input(None, vec![("ANTHROPIC_API_KEY", "sk-ant-aaaaaa")]);
        claude.catalog_id = Some("claude".into());
        claude.make_default = true;
        let added = save_agent(&app, claude.clone()).unwrap();
        assert_eq!(added.id, "claude");
        assert_eq!(added.settings.default_agent, "claude");
        assert_eq!(added.settings.agents[0].label, "Claude Code");
        assert_eq!(
            added.settings.agents[0].catalog_id.as_deref(),
            Some("claude")
        );

        // Adding the same catalog agent again updates it, keeping its secret.
        claude.env = vec![AcpEnvInput {
            name: "ANTHROPIC_API_KEY".into(),
            value: Some(String::new()),
        }];
        claude.make_default = false;
        let again = save_agent(&app, claude).unwrap();
        assert_eq!(again.settings.agents.len(), 1);
        assert!(has_secret(&app, "acp-env:claude:ANTHROPIC_API_KEY"));

        let custom = save_agent(&app, input(None, vec![("MY_TOKEN", "tok-123456")])).unwrap();
        assert_eq!(custom.id, "custom-1");
        assert_eq!(custom.settings.agents.len(), 2);

        // Removing the default: its secrets go, and built-in is the default.
        let after = remove_agent(&app, "claude").unwrap();
        assert_eq!(after.default_agent, BUILTIN);
        assert_eq!(after.agents.len(), 1);
        assert!(!has_secret(&app, "acp-env:claude:ANTHROPIC_API_KEY"));
        assert!(has_secret(&app, "acp-env:custom-1:MY_TOKEN"));
        assert!(remove_agent(&app, "claude").is_err());
        remove_agent(&app, "custom-1").unwrap();
        assert!(!has_secret(&app, "acp-env:custom-1:MY_TOKEN"));
        // Ids are reused only once free.
        assert_eq!(
            save_agent(&app, input(None, vec![])).unwrap().id,
            "custom-1"
        );
    }

    #[test]
    fn the_default_and_a_per_chat_pick_choose_the_agent() {
        let app = app();
        let exe = std::env::current_exe().unwrap();
        let mut a = input(None, vec![]);
        a.command = exe.to_string_lossy().to_string();
        let first = save_agent(&app, a.clone()).unwrap().id;
        let second = save_agent(&app, a).unwrap().id;
        // Built-in by default: a turn without a pick has no external agent.
        assert!(load(&app, "")
            .err()
            .unwrap()
            .to_string()
            .contains("Settings → AI"));
        set_default(&app, &second).unwrap();
        assert_eq!(load(&app, "").unwrap().id, second);
        // A chat's own pick wins over the default.
        assert_eq!(load(&app, &first).unwrap().id, first);
        assert!(load(&app, "gone")
            .err()
            .unwrap()
            .to_string()
            .contains("removed"));
        assert!(set_default(&app, "gone").is_err());
        set_default(&app, BUILTIN).unwrap();
        assert_eq!(default_agent(&app), BUILTIN);
        // A default pointing at a removed agent reads as built-in.
        set_default(&app, &first).unwrap();
        remove_agent(&app, &first).unwrap();
        assert_eq!(default_agent(&app), BUILTIN);

        remember_agent_name(&app, &second, "Fake Agent");
        assert_eq!(agent_label(&app, &second), "Fake Agent");
    }

    #[test]
    fn bad_input_is_refused() {
        let app = app();
        assert!(save_agent(&app, input(None, vec![("1BAD", "x")])).is_err());
        assert!(save_agent(&app, input(None, vec![("A", "x"), ("A", "y")])).is_err());
        let mut no_command = input(None, vec![]);
        no_command.command = " ".into();
        assert!(save_agent(&app, no_command).is_err());
        let mut unknown = input(None, vec![]);
        unknown.catalog_id = Some("nope".into());
        assert!(save_agent(&app, unknown).is_err());
        assert!(save_agent(&app, input(Some("missing"), vec![])).is_err());
        assert!(view(&app).agents.is_empty());
    }

    #[test]
    fn a_missing_command_says_how_to_fix_it() {
        let app = app();
        let mut i = input(None, vec![]);
        i.command = "definitely-not-a-real-agent-binary".into();
        let id = save_agent(&app, i).unwrap().id;
        let err = load(&app, &id).err().unwrap().to_string();
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
