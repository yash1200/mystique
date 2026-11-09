//! Host-facing ACP service: the agent list, detection, the Test button, and
//! turns.

use std::path::PathBuf;
use std::sync::Arc;

use mystique_agent::kit::{transcript, workspace_context, ChatHost};
use mystique_agent::{AgentEventSink, AgentRuntime, SendAgentMessage, SendAgentResult, TurnStart};
use mystique_core::{AppError, AppResult, AppServices, ToolCaller};
use serde::{Deserialize, Serialize};

use crate::catalog;
use crate::detect::{self, DetectedAcpAgent};
use crate::process::{Children, Launch};
use crate::protocol::{self, AgentHello, MCP_SERVER_NAME};
use crate::scrub::Scrubber;
use crate::session::{
    self, Launcher, McpAccess, ProbeOutcome, ProcessLauncher, TurnSpec, RUN_LIMIT,
};
use crate::settings::{
    self, AcpAgentSaved, AcpEnvInput, AcpSettingsView, SaveAcpAgent, ACP_PROVIDER, BUILTIN,
};

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
    /// The added agent that was tested.
    pub agent_id: String,
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    pub version: String,
    pub protocol_version: u16,
    /// Takes HTTP MCP servers.
    pub mcp_http: bool,
    pub mcp_sse: bool,
    /// How a turn reaches Mystique's tools: `http`, `stdio` (the bridge),
    /// or `None` when it cannot.
    #[serde(default)]
    pub mcp_transport: Option<String>,
    pub load_session: bool,
    /// Sign-in methods the agent offers (Mystique does not run them).
    pub auth_methods: Vec<String>,
    /// `false`: the agent asked for sign-in when a session was opened.
    /// `None`: not known (some agents only check at the first prompt).
    #[serde(default)]
    pub signed_in: Option<bool>,
    /// Why a turn would fail, if it would.
    #[serde(default)]
    pub problem: Option<String>,
}

impl AcpProbe {
    fn from_outcome(
        agent_id: &str,
        catalog_id: Option<&str>,
        outcome: &ProbeOutcome,
        bridge: Option<&std::path::Path>,
    ) -> Self {
        let hello = &outcome.hello;
        let route = protocol::mcp_route(hello, bridge);
        let sign_in = catalog_id
            .and_then(catalog::find)
            .map(|e| e.sign_in)
            .unwrap_or(
            "Run the agent once in a terminal and log in, or add its API key under Environment.",
        );
        let problem = match (&route, outcome.signed_in) {
            (Err(reason), _) => Some(reason.clone()),
            (Ok(_), Some(false)) => Some(format!("Not signed in. {sign_in}")),
            (Ok(_), _) => outcome.session_problem.clone(),
        };
        Self {
            agent_id: agent_id.to_string(),
            name: hello.name.clone(),
            title: hello.title.clone(),
            version: hello.version.clone(),
            protocol_version: hello.protocol_version,
            mcp_http: hello.mcp_http,
            mcp_sse: hello.mcp_sse,
            mcp_transport: route.ok().map(|r| r.label().to_string()),
            load_session: hello.load_session,
            auth_methods: hello.auth_methods.clone(),
            signed_in: outcome.signed_in,
            problem,
        }
    }
}

/// Settings and turns for external agents. Clones share the children.
#[derive(Clone)]
pub struct AcpService {
    app: Arc<AppServices>,
    workdir: PathBuf,
    children: Children,
    launcher: Arc<dyn Launcher>,
    /// The app binary, run as the stdio MCP bridge for agents without HTTP
    /// MCP. `None`: those agents are refused.
    bridge: Option<PathBuf>,
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
            bridge: None,
        }
    }

    /// Agents that cannot reach HTTP MCP start `program --mcp-stdio-bridge`
    /// (see [`crate::bridge`]). `program` is the app's own binary.
    pub fn with_bridge(mut self, program: PathBuf) -> Self {
        self.bridge = Some(program);
        self
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
            bridge: None,
        }
    }

    /// Settings → AI → Agent. Never loads the vault key.
    pub fn settings(&self) -> AcpSettingsView {
        settings::view(&self.app)
    }

    /// Add or change an agent. May load the vault key (a secret value).
    pub fn save_agent(&self, input: SaveAcpAgent) -> AppResult<AcpAgentSaved> {
        settings::save_agent(&self.app, input)
    }

    /// Remove an agent and its vault secrets.
    pub fn remove_agent(&self, id: &str) -> AppResult<AcpSettingsView> {
        settings::remove_agent(&self.app, id)
    }

    /// What new chats use: `builtin` or an agent id.
    pub fn set_default(&self, id: &str) -> AppResult<AcpSettingsView> {
        settings::set_default(&self.app, id)
    }

    /// Which catalog agents are on this machine. Blocking (it looks through
    /// folders and asks each CLI for its version, 1.5 s at most each, in
    /// parallel; the first call also asks the login shell for its PATH).
    pub fn detect(&self) -> Vec<DetectedAcpAgent> {
        let added = settings::view(&self.app).agents;
        let mut list = detect::detect();
        for agent in &mut list {
            agent.added_id = added
                .iter()
                .find(|a| a.catalog_id.as_deref() == Some(agent.catalog_id.as_str()))
                .map(|a| a.id.clone());
        }
        list
    }

    /// Add a detected catalog agent with one click: its launch command as
    /// found on this machine. Adding it again refreshes the command and
    /// keeps its environment. The first agent added becomes the default.
    /// Blocking.
    pub fn add_detected(&self, catalog_id: &str) -> AppResult<AcpAgentSaved> {
        let entry = catalog::find(catalog_id)
            .ok_or_else(|| AppError::Invalid(format!("Unknown agent `{catalog_id}`.")))?;
        let (command, args) = detect::launch_for(entry).map_err(AppError::Invalid)?;
        let current = settings::view(&self.app);
        let existing = current
            .agents
            .iter()
            .find(|a| a.catalog_id.as_deref() == Some(entry.id));
        let env = existing
            .map(|a| {
                a.env
                    .iter()
                    .map(|e| AcpEnvInput {
                        name: e.name.clone(),
                        // Empty keeps a saved secret.
                        value: e.value.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        settings::save_agent(
            &self.app,
            SaveAcpAgent {
                id: existing.map(|a| a.id.clone()),
                catalog_id: Some(entry.id.to_string()),
                label: None,
                command,
                args,
                env,
                make_default: current.agents.is_empty() && current.default_agent == BUILTIN,
            },
        )
    }

    /// Kill every running agent process now (app quit).
    pub fn kill_all(&self) -> usize {
        self.children.kill_all()
    }

    /// Agent processes alive now.
    pub fn running_agents(&self) -> usize {
        self.children.count()
    }

    fn launch(&self, requested: &str) -> AppResult<(Launch, settings::AgentConfig)> {
        let config = settings::load(&self.app, requested)?;
        std::fs::create_dir_all(&self.workdir)
            .map_err(|e| AppError::Internal(format!("could not create the agent's folder: {e}")))?;
        let cwd = self
            .workdir
            .canonicalize()
            .unwrap_or_else(|_| self.workdir.clone());
        Ok((
            Launch {
                program: config.program.clone(),
                args: config.args.clone(),
                env: config.env.clone(),
                cwd,
            },
            config,
        ))
    }

    /// Test button: start agent `id`, run `initialize`, open a session
    /// without MCP servers to see whether it is signed in, stop it. Loads
    /// secret environment values, so call it off the async hot path.
    pub async fn probe(&self, id: &str) -> AppResult<AcpProbe> {
        let this = self.clone();
        let requested = id.to_string();
        let (launch, config) = tokio::task::spawn_blocking(move || this.launch(&requested))
            .await
            .map_err(|e| AppError::Internal(format!("agent settings: {e}")))??;
        let scrub = Scrubber::new(config.secrets.clone());
        let outcome = session::probe(self.launcher.as_ref(), &launch, &scrub)
            .await
            .map_err(AppError::Invalid)?;
        let hello = &outcome.hello;
        settings::remember_agent_name(
            &self.app,
            &config.id,
            hello.title.as_deref().unwrap_or(&hello.name),
        );
        Ok(AcpProbe::from_outcome(
            &config.id,
            config.catalog_id.as_deref(),
            &outcome,
            self.bridge.as_deref(),
        ))
    }

    /// Save the user message and start an external-agent turn with the
    /// agent `req.model` names (empty: the default agent). The chat
    /// remembers the agent: its `model` is the agent's id. Returns at once;
    /// the reply streams to `sink` and ends with `done` or `error`. Stop is
    /// `AgentRuntime::cancel`, as for the built-in agent. Blocking (SQLite,
    /// vault); call inside a Tokio runtime.
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
        let (launch, config) = self.launch(&req.model)?;
        let agent_id = config.id.clone();
        let TurnStart {
            chat,
            user,
            prior,
            text,
            claim,
        } = runtime.open_turn(&self.app, &req, ACP_PROVIDER, &agent_id)?;

        let mut secrets = config.secrets;
        secrets.push(mcp.token.clone());
        let scrub = Scrubber::new(secrets);
        let spec = TurnSpec {
            launch,
            prompt: vec![context_block(&req, &prior), text],
            mcp,
            bridge: self.bridge.clone(),
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
        let on_hello = Box::new(move |hello: &AgentHello| {
            let name = hello.title.as_deref().unwrap_or(&hello.name);
            settings::remember_agent_name(&app, &agent_id, name);
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
