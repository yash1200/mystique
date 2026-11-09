# External agents (ACP)

Mystique can run an external coding agent in the agent pane instead of the
built-in agent. It speaks the [Agent Client Protocol](https://agentclientprotocol.com)
(ACP). **Mystique is the ACP client**: it starts the agent, sends your
message, shows the streamed reply and tool calls, and answers the agent's
requests. The agent gets Mystique's database tools and nothing else from
Mystique, behind the same access levels and approval dock as any MCP client.

Code: `crates/mystique-acp` (protocol, process, permissions, mapping, tests),
`src-tauri/src/ipc/agent.rs` (`send_agent_message` with `provider: "acp"`,
`get_acp_settings`, `save_acp_settings`, `test_acp_agent`), Settings → AI →
Agent (`src/lib/features/settings/ui/AgentBackendSettings.svelte`).

## Protocol revision

- **ACP protocol version 1** (the stable revision; `initialize` sends
  `protocolVersion: 1` and refuses an agent that answers with another).
- Wire types from **`agent-client-protocol-schema` = 1.16.0**, pinned
  exactly, default features off, no `unstable_*` features. Upgrading it is a
  deliberate change: read the schema changelog, then rerun the ACP tests.
- Not the SDK crate (`agent-client-protocol` 5.x). The SDK brings its own
  connection runtime, proxy chains and component model. Mystique needs a
  small client: one child process per turn, on the app's Tokio runtime,
  with the existing cancel path (`cancel_agent_message` → the run's
  `CancellationToken`) able to kill it. The schema crate gives exact,
  versioned types; `crates/mystique-acp/src/rpc.rs` is ~250 lines of
  newline-delimited JSON-RPC.
- Transport: stdio, one JSON-RPC message per line (ACP's stdio transport).
  A line over 32 MiB ends the connection.

Methods Mystique calls: `initialize`, `session/new`, `session/prompt`,
`session/cancel` (notification). Methods it answers: `session/update`
(notification), `session/request_permission`. Everything else the agent asks
(`fs/*`, `terminal/*`, `elicitation/*`, …) gets JSON-RPC error `-32601`.

## The first harness: Claude Code

One harness is supported and documented: **Claude Code through its ACP
adapter**, npm package `@agentclientprotocol/claude-agent-acp` (formerly
`@zed-industries/claude-code-acp` / `claude-agent-acp`; the binary is
`claude-agent-acp`).

Why Claude Code rather than Gemini CLI:

- Mystique's MCP server is a local **Streamable HTTP** server with a bearer
  token. ACP passes MCP servers in `session/new`; `stdio` is the only
  transport every agent must support, `http` is an optional capability
  (`agentCapabilities.mcpCapabilities.http`).
- The Claude adapter advertises `mcpCapabilities.http: true` and connects to
  HTTP MCP servers with the headers it is given, so Mystique passes its own
  server directly: `{ type: "http", name: "mystique", url, headers:
  [Authorization: Bearer <token>] }`. No bridge process, no token on a
  command line.
- Gemini CLI's ACP mode (`gemini --acp`, earlier `--experimental-acp`) did
  not document HTTP MCP support in `session/new` when this was written; it
  would need a stdio MCP bridge, which Mystique does not ship. Any agent that
  advertises `mcpCapabilities.http` should work, but only Claude Code is
  tested and documented.

An agent without `mcpCapabilities.http` is refused with a clear message
(Test shows it too).

### Install and configure

Mystique never installs anything. Install the adapter yourself, either:

- the standalone binary from the
  [releases](https://github.com/agentclientprotocol/claude-agent-acp/releases)
  (no Node needed): command `/path/to/claude-agent-acp`, no arguments; or
- npm: command `npx`, arguments `-y @agentclientprotocol/claude-agent-acp`
  (Node 18+ on PATH).

Sign in once: run `claude` in a terminal and log in, or set
`ANTHROPIC_API_KEY` under Environment (it is stored in the vault, see
below). Mystique does not run the agent's sign-in flow; Test lists the
sign-in methods the agent offers.

In Mystique:

1. Settings → MCP: turn the MCP server on, and set the access level of the
   connections the agent may use (MCP's levels apply, see Security).
2. Settings → AI: enable the query agent. Under **Agent**, pick
   **External (ACP)**, enter the command and arguments, add environment
   variables if needed, press **Test**. Test starts the agent, runs
   `initialize`, shows its name, version, protocol version, whether it can
   reach MCP over HTTP and its sign-in methods, then stops it.
3. Open a query tab and the agent pane. The header reads
   `Agent · Claude Code` (or the agent's name); the model picker is replaced
   by the agent's name (the agent picks its own model).

Apps started from Finder do not see your shell's `PATH`. Mystique adds
`/opt/homebrew/bin` and `/usr/local/bin`; otherwise use a full path for the
command, or set `PATH` under Environment.

## What Mystique offers and declines

| Capability | Mystique's answer |
|---|---|
| `clientCapabilities.fs.readTextFile` / `writeTextFile` | `false`: declined |
| `clientCapabilities.terminal` | `false`: declined |
| `fs/*`, `terminal/*` requests anyway | error `-32601` |
| MCP servers in `session/new` | exactly one: Mystique's (HTTP, bearer token) |
| Working directory (`cwd`) | an empty folder, `<app data>/acp-agent` |
| Prompt content | text blocks only |
| `session/load`, modes, config options, auth | not used |
| `elicitation` | not declared |

The prompt is two text blocks: Mystique's context (a short contract, the
connection name, engine, database, **connection id** for the tools, the
editor buffer, `@`-referenced tables, and earlier turns of the chat as a
transcript), then your words. Each turn starts a fresh agent process and
session; the chat history travels in the transcript.

## Turn lifecycle

1. Spawn the command (no shell) in its own process group, `cwd` = the empty
   folder, stdin/stdout piped for ACP, stderr captured.
2. `initialize` (90 s limit). Refuse protocol ≠ 1 or no HTTP MCP.
3. `session/new` with Mystique's MCP server (90 s limit).
4. `session/prompt`; serve `session/update` and `session/request_permission`
   until the reply (10 min limit for the whole turn: a write may wait 90 s in
   the dock, and agents plan before calling tools).
5. Close stdin, give the agent 300 ms to exit, kill the process group.

**Stop** (and the time limit) sends `session/cancel`, keeps serving the agent
for 500 ms so it can end the turn, then kills the whole process group
(`npx` → node → the Claude CLI). Stop returns within about a second even if
the agent ignores the cancel. On **app quit** every run is cancelled and every
agent process group is killed synchronously.

One live run per chat, shared with the built-in agent's run registry: Stop is
the same `cancel_agent_message`.

## Event mapping (ACP → `AgentEvent`)

No new `AgentEvent` variants. The same 75 ms delta batching
(`mystique_agent::kit::EventStream`) and the same redaction apply.

| ACP `session/update` | `AgentEvent` |
|---|---|
| `agent_message_chunk` (text; resource link as `[name](uri)`) | `textDelta` |
| `agent_thought_chunk` (text) | `reasoningDelta` |
| `plan` (entries) | `reasoningDelta` with a checklist (`- [x]`, `- [~]`, `- [ ]`), only when it changed |
| `tool_call` | `toolCallStart { id, name, args }` (and `toolCallEnd` if it arrives finished) |
| `tool_call_update` with status `completed` / `failed` | `toolCallEnd { id, ok, summary, messageId }`; the tool card is saved |
| a dock request (caller `mcp`, this connection) while a Mystique tool call runs | `toolApprovalPending { id, approvalId, sql, needed }` |
| `user_message_chunk`, `available_commands_update`, `current_mode_update`, `config_option_update`, `session_info_update`, `usage_update`, others | ignored |

| `session/prompt` stop reason | Terminal events |
|---|---|
| `end_turn` | reply saved → `done { messageId, tokens: 0 }` (empty reply → `error { provider }`) |
| `max_tokens` | reply saved with a note → `done` |
| `max_turn_requests` | `exhausted { turns: 0 }`, reply saved with a note → `done` (the pane offers Continue) |
| `refusal` | partial kept → `error { provider }` |
| `cancelled`, or Stop | partial kept → `error { cancelled }` |
| time limit | partial kept → `error { timeout }` |
| agent exits, bad reply, `-32000` auth required | partial kept → `error { provider }` with the reason and the agent's last stderr lines (masked) |

Tool names: a call is Mystique's when its `name`, `_meta.*.toolName` or title
is server-qualified with `mystique` (`mcp__mystique__execute_query` from
Claude Code, or `mystique/…`, `mystique__…`, `mystique.…`, `mystique:…`) and
names one of Mystique's tools; the card then shows the plain tool name
(`execute_query`). Other calls keep their title. Token counts are 0 (ACP v1
reports none per turn).

Chats persist like built-in chats (user message, tool cards, reply with its
draft) and are tagged `provider: "acp"`, `model: <agent name>`.

## Permission policy

`session/request_permission`:

- A call to **one of Mystique's MCP tools** → the agent's **allow once**
  option. Never an "always" option. Allowing here grants nothing extra: the
  call still reaches the MCP server, where access levels decide and the
  approval dock asks for high-risk writes.
- **Anything else** (the agent's own Bash, Edit, Write, WebFetch, other MCP
  servers, a bare tool name that any server could use) → **reject once**, or
  `cancelled` if the agent offers no reject option.

Each decision is logged at info with the (masked) tool name, never its
arguments.

What this does not cover: tools the agent runs *without* asking. Claude Code
may read files in its working directory without a prompt — that folder is
empty. It also loads MCP servers from your own Claude Code settings; their
calls are rejected when the agent asks, but servers you set to auto-approve
in Claude Code are outside Mystique's control.

## Security model

- **The agent reaches databases only through Mystique's MCP server**, so it
  is an MCP caller: **MCP's access levels** (Settings → MCP → Access) apply,
  and its writes wait in the **approval dock** like any MCP client's (caller
  `mcp`). ACP is not a bypass: `a_write_waits_in_the_dock_and_is_denied_on_timeout`
  runs a write end to end, sees it in the dock and denied at the timeout,
  and checks nothing ran.
- The MCP server must be **running**; the turn is refused otherwise. The
  token is the existing MCP token, sent only in the `Authorization` header of
  `session/new` (never on a command line or in the environment).
- **No file system or terminal** through Mystique; the agent's working
  directory is an empty app-data folder.
- **Masking.** The MCP token and every secret environment value are masked
  (`[redacted]`) before anything reaches the window, a log line or a saved
  row: streamed text (held back only while a chunk could be the start of a
  secret, so a secret split across chunks is still caught), tool arguments,
  results and titles, error messages, and the agent's stderr. Then the shared
  redaction runs (`AgentEvent::redacted`, `mystique_core::redact`: URI
  passwords, `key=value` secrets, passwords in SQL). The agent's stderr goes
  to the log at debug level (`mystique_acp::agent_stderr`), masked.
- **Environment values** whose names look secret (`KEY`, `TOKEN`, `SECRET`,
  `PASSWORD`, `PASSWD`, `CREDENTIAL`, `AUTH`) are sealed in the vault as
  `ai:acp-env:<NAME>` and never sent back to the window; others are plain
  settings. Removing a variable removes its secret.
- The agent process runs as you, with your environment plus what you set.
  It is your agent: Mystique limits what *Mystique* gives it, not what the
  program can do on its own.

## Tests

`cargo test -p mystique-acp` (no real agent needed; a fake ACP agent speaks
over in-memory pipes against the real MCP server; process tests use
`/bin/sh`):

- `the_test_button_initializes_and_declines_files_and_terminal`
- `a_turn_streams_text_and_a_tool_call_through_mystique_mcp` (session setup,
  text/thought/plan mapping, an `fs/read_text_file` refused, a real
  `list_objects` over MCP, cards and reply saved, chat tagged)
- `permission_is_allowed_once_for_mystique_tools_and_rejected_for_the_rest`
- `stop_mid_turn_cancels_the_session_and_ends_within_a_second`
- `stop_kills_the_agent_process_group`, `app_quit_kills_running_agents`
- `a_write_waits_in_the_dock_and_is_denied_on_timeout`
- `planted_secrets_never_reach_the_stream_logs_or_saved_rows`,
  `the_agent_s_stderr_is_logged_masked`
- unit tests for the protocol messages, JSON-RPC routing, permission policy,
  masking and settings.

## Manual test (acceptance for #94)

Against a local build (`pnpm tauri:dev`), with Claude Code's adapter
installed and signed in:

1. Settings → MCP: turn the server on. Access: set a test connection to
   `write`, and turn on "ask me for high-risk writes" for it.
2. Settings → AI: enable the agent; Agent → External (ACP); command `npx`,
   arguments `-y @agentclientprotocol/claude-agent-acp` (or the binary's
   full path). Press **Test**: it shows the agent's name and version,
   "ACP v1 · MCP over HTTP: yes".
3. Open a query tab on that connection, open the agent pane. The header
   shows `Agent · <agent name>`.
4. Ask "What tables are there?". Text streams; a tool card
   ("Listing objects") appears and finishes; the reply ends with a SQL
   draft you can apply.
5. Ask "Insert a test row into <table>". The `execute_query` card shows
   "waiting"; the approval dock shows the statement for MCP. Deny it: the
   card fails with the denial, nothing was written.
6. Ask for something long ("Describe every table, one by one") and press
   **Stop** while a tool call runs. The run ends within about a second as
   Stopped; `ps` shows no `claude-agent-acp` / `claude` process left.
7. Ask it to run a shell command or read a file: the pane shows the tool
   call failing (rejected); the log (`MYSTIQUE_LOG=debug`) has
   "rejected a tool call that is not Mystique's".
8. Quit Mystique during a turn: no agent process remains.

## Deferred

- Other harnesses (Gemini CLI needs a stdio MCP bridge; Codex, others).
  Only Claude Code is documented.
- Long-lived sessions per chat (`session/load` / keeping the process):
  each turn starts a fresh process and replays the chat as a transcript.
- Images and resources in prompts; diffs and terminals in tool content.
- Session modes and config options (the agent's own model picker).
- Running the agent's sign-in flow (`authenticate`) from Mystique.
- Token usage (`usage_update`, `unstable_end_turn_token_usage`).
- Agent-level (not MCP-level) access levels for the external agent: it would
  need the MCP server to know the caller is the in-app pane.
