//! Host-facing ACP service: settings, the Test button, and turns.

use std::path::PathBuf;
use std::sync::Arc;

use mystique_agent::kit::{transcript, workspace_context, ChatHost};
use mystique_agent::{AgentEventSink, AgentRuntime, SendAgentMessage, SendAgentResult, TurnStart};
use mystique_core::{AppError, AppResult, AppServices, ToolCaller};
use serde::{Deserialize, Serialize};

use crate::process::{Children, Launch};
use crate::protocol::{AgentHello, MCP_SERVER_NAME};
use crate::scrub::Scrubber;
use crate::session::{self, Launcher, McpAccess, ProcessLauncher, TurnSpec, RUN_LIMIT};
use crate::settings::{self, AcpSettingsView, SaveAcpSettings, ACP_PROVIDER, KIND_ACP};

/// Earlier turns sent with each prompt (characters).
const TRANSCRIPT_BUDGET: usize = 48_000;

/// What the agent's working directory is called under the app data folder.
const WORKDIR: &str = "acp-agent";

/// What Mystique tells an external agent before the user's words.
const CONTRACT: &str = "\
You are working inside Mystique, a desktop database studio, on one database connection the user has open.\n\
\n\
- Use only the `mystique` MCP tools to look at or change the database. Pass the connection id below as `connection_id`. Check names with describe_object or get_schema_dbml; never invent tables or columns. Use sample_rows (at most 20 rows), explain_query and infer_fields instead of SELECT *.\n\
- Mystique gives you no file system or shell access. Other tools are refused.\n\
- The user's access levels decide what may run. A denied write ran nothing: say so and do not retry it. A write may wait for the user's approval in Mystique.\n\
- When a query is the answer, finish with exactly one fenced ```sql or ```mql block holding the complete draft; the user applies it to the editor.\n\
- Keep it short. Never ask for hosts, passwords or schema pastes.";

/// What the Test button shows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct AcpProbe {
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    pub version: String,
    pub protocol_version: u16,
    /// Can reach Mystique's MCP server (HTTP transport).
    pub mcp_http: bool,
    pub load_session: bool,
    /// Sign-in methods the agent offers (Mystique does not run them).
    pub auth_methods: Vec<String>,
    /// Why a turn would fail, if it would.
    #[serde(default)]
    pub problem: Option<String>,
}

impl AcpProbe {
    fn from_hello(hello: &AgentHello) -> Self {
        Self {
            name: hello.name.clone(),
            title: hello.title.clone(),
            version: hello.version.clone(),
            protocol_version: hello.protocol_version,
            mcp_http: hello.mcp_http,
            load_session: hello.load_session,
            auth_methods: hello.auth_methods.clone(),
            problem: crate::protocol::unsupported(hello),
        }
    }
}

/// Settings and turns for the external agent. Clones share the children.
#[derive(Clone)]
pub struct AcpService {
    app: Arc<AppServices>,
    workdir: PathBuf,
    children: Children,
    launcher: Arc<dyn Launcher>,
}

impl AcpService {
    /// `data_dir` is the app data folder: the agent runs in an empty
    /// folder inside it.
    pub fn new(app: Arc<AppServices>, data_dir: impl Into<PathBuf>) -> Self {
        let children = Children::default();
        Self {
            app,
            workdir: data_dir.into().join(WORKDIR),
            launcher: Arc::new(ProcessLauncher {
                children: children.clone(),
            }),
            children,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_launcher(
        app: Arc<AppServices>,
        workdir: PathBuf,
        launcher: Arc<dyn Launcher>,
    ) -> Self {
        Self {
            app,
            workdir,
            children: Children::default(),
            launcher,
        }
    }

    /// Settings → AI → Agent. Never loads the vault key.
    pub fn settings(&self) -> AcpSettingsView {
        settings::view(&self.app)
    }

    /// May load the vault key (a secret environment value).
    pub fn save_settings(&self, input: SaveAcpSettings) -> AppResult<AcpSettingsView> {
        settings::save(&self.app, input)
    }

    /// True when the agent pane runs the external agent.
    pub fn active(&self) -> bool {
        settings::kind(&self.app) == KIND_ACP
    }

    /// Kill every running agent process now (app quit).
    pub fn kill_all(&self) -> usize {
        self.children.kill_all()
    }

    /// Agent processes alive now.
    pub fn running_agents(&self) -> usize {
        self.children.count()
    }

    fn launch(&self) -> AppResult<(Launch, Vec<String>)> {
        let config = settings::load(&self.app)?;
        std::fs::create_dir_all(&self.workdir)
            .map_err(|e| AppError::Internal(format!("could not create the agent's folder: {e}")))?;
        let cwd = self
            .workdir
            .canonicalize()
            .unwrap_or_else(|_| self.workdir.clone());
        Ok((
            Launch {
                program: config.program,
                args: config.args,
                env: config.env,
                cwd,
            },
            config.secrets,
        ))
    }

    /// Test button: start the agent, run `initialize`, stop it. Loads
    /// secret environment values, so call it off the async workers' hot path.
    pub async fn probe(&self) -> AppResult<AcpProbe> {
        let this = self.clone();
        let (launch, secrets) = tokio::task::spawn_blocking(move || this.launch())
            .await
            .map_err(|e| AppError::Internal(format!("agent settings: {e}")))??;
        let scrub = Scrubber::new(secrets);
        let hello = session::probe(self.launcher.as_ref(), &launch, &scrub)
            .await
            .map_err(AppError::Invalid)?;
        settings::remember_agent_name(&self.app, hello.title.as_deref().unwrap_or(&hello.name));
        Ok(AcpProbe::from_hello(&hello))
    }

    /// Save the user message and start an external-agent turn. Returns at
    /// once; the reply streams to `sink` and ends with `done` or `error`.
    /// Stop is `AgentRuntime::cancel`, as for the built-in agent. Blocking
    /// (SQLite, vault); call inside a Tokio runtime.
    pub fn send(
        &self,
        runtime: &AgentRuntime,
        mcp: McpAccess,
        req: SendAgentMessage,
        sink: AgentEventSink,
    ) -> AppResult<SendAgentResult> {
        if mcp.url.trim().is_empty() || mcp.token.trim().is_empty() {
            return Err(AppError::Invalid(
                "The external agent reaches your databases through Mystique's MCP server. Turn it on in Settings → MCP.".into(),
            ));
        }
        let (launch, mut secrets) = self.launch()?;
        let label = settings::agent_label(&self.app);
        let TurnStart {
            chat,
            user,
            prior,
            text,
            claim,
        } = runtime.open_turn(&self.app, &req, ACP_PROVIDER, &label)?;

        secrets.push(mcp.token.clone());
        let scrub = Scrubber::new(secrets);
        let spec = TurnSpec {
            launch,
            prompt: vec![context_block(&req, &prior), text],
            mcp,
            scrub,
            host: ChatHost {
                app: Arc::clone(&self.app),
                chat_id: chat.id.clone(),
                connection_id: req.connection_id.clone(),
                caller: ToolCaller::Mcp,
            },
            limit: RUN_LIMIT,
        };
        let launcher = Arc::clone(&self.launcher);
        let app = Arc::clone(&self.app);
        let chat_id = chat.id.clone();
        let on_hello = Box::new(move |hello: &AgentHello| {
            let name = hello.title.as_deref().unwrap_or(&hello.name).trim();
            if name.is_empty() {
                return;
            }
            settings::remember_agent_name(&app, name);
            // The chat is tagged with the agent's own name from now on.
            let _ = app
                .agent_chats()
                .update_meta(&chat_id, None, None, Some(name));
        });
        let token = claim.token().clone();
        tokio::spawn(async move {
            session::execute(launcher, spec, token, sink, on_hello).await;
            drop(claim);
        });
        Ok(SendAgentResult { chat, user })
    }
}

/// Mystique's context for the turn: the contract, the connection, the
/// editor, referenced tables, and the earlier turns.
fn context_block(req: &SendAgentMessage, prior: &[mystique_core::AgentMessage]) -> String {
    let database = req.database.as_deref().map(str::trim).unwrap_or("");
    let editor = req.editor_query.as_deref().unwrap_or("");
    let mut out = format!(
        "{CONTRACT}\n\n{}\nConnection id (for the `{MCP_SERVER_NAME}` tools): {}",
        workspace_context(&req.connection_name, &req.engine, database, editor),
        req.connection_id
    );
    let mentioned: Vec<String> = req
        .mentions
        .iter()
        .filter(|m| !m.name.trim().is_empty())
        .take(8)
        .map(
            |m| match m.schema.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(schema) => format!("{schema}.{}", m.name.trim()),
                None => m.name.trim().to_string(),
            },
        )
        .collect();
    if !mentioned.is_empty() {
        out.push_str("\nTables the user referenced with @: ");
        out.push_str(&mentioned.join(", "));
    }
    let earlier = transcript(prior, TRANSCRIPT_BUDGET);
    if !earlier.is_empty() {
        out.push_str("\n\nEarlier in this chat:\n");
        out.push_str(&earlier);
    }
    out
}
