# External agents (ACP)

Mystique can run an external coding agent in the agent pane instead of the
built-in agent. It speaks the [Agent Client Protocol](https://agentclientprotocol.com)
(ACP). **Mystique is the ACP client**: it starts the agent, sends your
message, shows the streamed reply and tool calls, and answers the agent's
requests. The agent gets Mystique's database tools and nothing else from
Mystique, behind the same access levels and approval dock as any MCP client.

Code: `crates/mystique-acp` (catalog, detection, stdio MCP bridge, protocol,
process, permissions, mapping, settings, tests), `src-tauri/src/main.rs`
(bridge mode), `src-tauri/src/ipc/agent.rs` (`send_agent_message` with
`provider: "acp"`, `get_acp_settings`, `save_acp_agent`, `remove_acp_agent`,
`set_default_agent`, `detect_acp_agents`, `add_detected_acp_agent`,
`test_acp_agent`), Settings → AI → Agent
(`src/lib/features/settings/ui/AgentBackendSettings.svelte`,
`AcpAgentForm.svelte`), the agent pane's agent menu
(`src/lib/features/workspace/agent/ui/AgentChatPane.svelte`).

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

## Supported agents

The catalog is a table in Rust, `crates/mystique-acp/src/catalog.rs`, one
entry per agent with its source URL. To add or fix an agent, edit the entry
and this table; `catalog::tests` checks every entry has a source URL, a
launch spec, an install hint and a unique id and binary names.

Researched November 2026 from the ACP registry
(<https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json>,
repo [agentclientprotocol/registry](https://github.com/agentclientprotocol/registry)),
the [ACP agents page](https://agentclientprotocol.com/overview/agents),
[Zed's external agents docs](https://zed.dev/docs/ai/external-agents) and
each vendor's docs. "MCP" is what the agent accepts in `session/new` as
documented (or, for Antigravity and Grok Build, as seen in their
`initialize` on a real machine); at run time the agent's own `initialize`
decides (see [MCP transport](#mcp-transport)).

| Agent | Detected by | ACP launch | MCP | ACP | Source |
|---|---|---|---|---|---|
| Claude Code | `claude-agent-acp` / `claude-code-acp` (adapter), else `claude` + npx/bunx | adapter: `claude-agent-acp`, or `npx -y @agentclientprotocol/claude-agent-acp` | http, sse, stdio | confirmed | [claude-agent-acp](https://github.com/agentclientprotocol/claude-agent-acp) |
| Codex | `codex-acp` (adapter), else `codex` + npx/bunx | adapter: `codex-acp`, or `npx -y @agentclientprotocol/codex-acp` | http, stdio | confirmed | [codex-acp](https://github.com/agentclientprotocol/codex-acp) |
| Gemini CLI | `gemini` | `gemini --acp` (older: `--experimental-acp`) | http, sse, stdio | confirmed | [acp-mode.md](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/acp-mode.md) |
| Antigravity CLI ("agy") | `agy` | `agy acp` (agy 1.5+) | stdio only → bridge | confirmed | [antigravity.google/docs/cli/acp](https://antigravity.google/docs/cli/acp/) |
| Grok Build ("grok build") | `grok` | `grok agent stdio` | http, sse, stdio | confirmed | [docs.x.ai headless](https://docs.x.ai/build/cli/headless-scripting) |
| Cursor CLI | `cursor-agent` | `cursor-agent acp` | unknown (reads `.cursor/mcp.json`) | confirmed | [cursor.com/docs/cli/acp](https://cursor.com/docs/cli/acp) |
| OpenCode | `opencode` | `opencode acp` | http, stdio | confirmed | [anomalyco/opencode](https://github.com/anomalyco/opencode) |
| goose | `goose` | `goose acp` | http, stdio | confirmed | [goose zed guide](https://github.com/aaif-goose/goose/blob/main/documentation/docs/guides/zed.md) |
| Kiro CLI | `kiro-cli` | `kiro-cli acp` | unknown | confirmed | [kiro.dev/docs/cli/acp](https://kiro.dev/docs/cli/acp/) |
| Qwen Code | `qwen` | `qwen --acp` | http, sse, stdio | confirmed | [integration-zed.md](https://github.com/QwenLM/qwen-code/blob/main/docs/users/integration-zed.md) |
| Auggie | `auggie` | `auggie --acp` | unknown | confirmed | [docs.augmentcode.com/cli/acp](https://docs.augmentcode.com/cli/acp) |
| GitHub Copilot CLI | `copilot` | `copilot --acp` (public preview) | unknown | confirmed | [docs.github.com acp-server](https://docs.github.com/en/copilot/reference/acp-server) |
| Kimi Code | `kimi` | `kimi acp` | http, sse, stdio | confirmed | [kimi-acp reference](https://moonshotai.github.io/kimi-code/en/reference/kimi-acp) |
| Mistral Vibe | `vibe-acp` | `vibe-acp` | stdio (none advertised) → bridge | confirmed | [acp-setup.md](https://github.com/mistralai/mistral-vibe/blob/main/docs/acp-setup.md) |
| Cline | `cline` | `cline --acp` | unknown | confirmed | [docs.cline.bot/usage/acp](https://docs.cline.bot/usage/acp) |
| Junie | `junie` | `junie --acp=true` | unknown | confirmed | [junie-cli-acp](https://junie.jetbrains.com/docs/junie-cli-acp.html) |
| Factory Droid | `droid` | `droid exec --output-format acp` | unknown | confirmed | [docs.factory.com zed](https://docs.factory.com/integrations/zed.md) |
| Qoder CLI | `qodercli` / `qoder` | `qodercli --acp` | http, sse, stdio | confirmed | [docs.qoder.com/cli/acp](https://docs.qoder.com/cli/acp) |
| Amp | `amp-acp` (adapter), else `amp` + npx/bunx | community adapter: `amp-acp`, or `npx -y amp-acp` | unknown | confirmed (adapter) | [amp-acp](https://github.com/tao12345666333/amp-acp) |
| OpenHands | `openhands` | `openhands acp` (CLI no longer maintained) | unknown | confirmed | [OpenHands ACP](https://docs.openhands.dev/openhands/usage/run-openhands/acp) |

Sign-in and install, per agent, are in the catalog (`sign_in`, `install`)
and shown in the UI. In short: adapters for Claude Code, Codex and Amp use
the CLI's own login (`claude`, `codex login`, `amp login`) or an API key
(`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`/`CODEX_API_KEY`, `AMP_API_KEY`);
native agents use their CLI's login (`agy`, `grok login`,
`cursor-agent login`, `opencode auth login`, `goose configure`,
`kiro-cli login`, `auggie login`, `/login` in `copilot` or `kimi`, …) or
their key (`GEMINI_API_KEY`, `XAI_API_KEY`, `CURSOR_API_KEY`,
`MISTRAL_API_KEY`, …). Mystique never runs a sign-in flow; Test says when an
agent is signed out and how to sign in.

The user's examples, plainly:

- **"agy"** is Google's Antigravity CLI. `agy acp` is documented and was
  checked on a real install (agy 1.5.3 answers `initialize` as "Antigravity
  ACP Server"). It advertises no HTTP MCP, so it uses the stdio bridge. Older
  agy versions have no `acp` subcommand.
- **"grok build"** is xAI's official Grok Build CLI (`grok`). `grok agent
  stdio` is its ACP mode (docs.x.ai, and checked on grok 1.0.79: protocol 1,
  `mcpCapabilities {http, sse}`). The community `grok-cli` (npm `grok-dev`)
  also installs a `grok` binary and does **not** speak ACP; Test would show
  that.
- **Cursor**: `cursor-agent acp` is documented. Detection looks for
  `cursor-agent` only, because Grok Build also installs a binary called
  `agent` (Cursor's newer name). Cursor reads MCP servers from
  `.cursor/mcp.json`; whether it uses the one Mystique passes in
  `session/new` is not documented — Test and a first turn show it.

No catalog agent is currently "not confirmed". The UI still supports it: an
entry with `confirmed: false` shows as "ACP support not confirmed" with no
Add button (Custom agent… still works).

Kept out of the catalog (in the registry, but niche, quarantined or needing
extra arguments such as an agent file): Docker Agent (`docker agent serve
acp <agent.yaml>`), fast-agent (quarantined in the registry), Kilo, Pi,
Poolside, Code Assistant, Devin, IBM Bob, and the rest of the registry's
long tail. Each runs as a Custom agent.

## Detection

`crates/mystique-acp/src/detect.rs`. An app started from Finder gets a
minimal `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`), so detection searches, in
order, without duplicates:

1. the app's `PATH`;
2. the login shell's `PATH`: `$SHELL -l -i -c 'printf …"$PATH"…'`, run once
   per app session (cached), 2 s limit, stdin closed, its own process group
   (killed whole on timeout), output read between markers so a chatty
   profile cannot corrupt it, and used **only** as a list of folders.
   Skipped silently on any failure, and on Windows;
3. `/opt/homebrew/bin`, `/usr/local/bin`, `/home/linuxbrew/.linuxbrew/bin`,
   `~/.local/bin`, `~/.npm-global/bin`, `~/.bun/bin`, `~/.cargo/bin`,
   `~/.volta/bin`, `~/.deno/bin`, `~/Library/pnpm`, `~/.local/share/pnpm`,
   `~/.opencode/bin`, `~/.grok/bin`, the newest `~/.nvm/versions/node/v*/bin`,
   fnm's default alias; on Windows `%APPDATA%\npm`, `%LOCALAPPDATA%\pnpm`,
   `%LOCALAPPDATA%\Volta\bin`, `~\scoop\shims` (with `.exe`/`.cmd`/`.bat`);
4. npm's global bin: `npm prefix -g` (+`/bin`), asked only when `npm` itself
   was found, with the folders above as its `PATH` (npm needs node), 1.5 s
   limit.

Per catalog entry it reports **Installed** (its ACP entry point is a local
binary: the native CLI, or the adapter's own binary), **Adapter** (the CLI
is here and its adapter runs through `npx -y <pkg>`, else `bunx <pkg>`),
**ACP support not confirmed** (CLI found, entry not confirmed), or **Not
installed** (with the install command and homepage). Installed entries are
asked `--version` (1.5 s limit each, in parallel, stdin closed, own process
group); a slow answer just leaves the version blank. Nothing is ever
installed and `npx` is never run during detection (it could download).

The same folders, with the program's own folder first, make up the `PATH`
an agent is started with (unless you set `PATH` under its Environment), so
`npx` from nvm finds `node`.

Real output on the developer's Mac with a Finder-like `PATH`
(`cargo run -p mystique-acp --example detect_agents`):

```
Claude Code          Adapter      2.1.319 (Claude Code)        ~/.nvm/versions/node/v26.12.1/bin/npx -y @agentclientprotocol/claude-agent-acp
Codex                Adapter      codex-cli 0.167.0            ~/.nvm/versions/node/v26.12.1/bin/npx -y @agentclientprotocol/codex-acp
Antigravity CLI      Installed    1.5.3                        ~/.local/bin/agy acp
Grok Build           Installed    grok 1.0.79 (ce7632526177)   ~/.grok/bin/grok agent stdio
(16 more: Missing)   20 agents checked in 1.35s
```

## Settings → AI → Agent

- **Built-in** first (the default until you pick another).
- **Your agents**: each added agent with its command line, Default (or
  Make default), Test, Edit (the manual form) and Remove.
- **Detected on this Mac**: icon, name, version, path, status. **Add** fills
  the launch command from the catalog and detection, saves it, and runs
  Test right away, inline: agent name and version, `ACP v1 · MCP over HTTP`
  or `MCP through the stdio bridge`, `signed in` / `not signed in`, and the
  problem if any (e.g. "Not signed in. Run `claude` once in a terminal and
  log in, or set ANTHROPIC_API_KEY."). The first agent you add becomes the
  default. **Rescan** looks again (the login-shell `PATH` is not re-asked).
- **Not installed (n)**, collapsed: each with Copy command (the install
  command) and Copy link (homepage). Mystique never installs anything.
- **Custom agent…**: the manual form (command, arguments, environment rows,
  Save, Save and test) for anything not in the catalog.

**Test** starts the agent, runs `initialize`, then opens a session with
**no MCP server** to see whether it is signed in (`-32000` auth required →
"not signed in"), and stops it (30 s limit for that session). Some agents
only check sign-in at the first prompt; then Test cannot tell.

The **agent pane header** reads `Agent · <menu>`: Built-in and every added
agent. The pick is remembered **per chat**: an external agent's chat is
saved with `provider: "acp"` and `model: <agent id>`; a built-in chat keeps
its provider and model. A new chat starts on the default. A chat whose agent
was removed (or an older external chat, whose `model` was the agent's name)
falls back to the default.

### Storage and migration

- `ai.acp.agents`: JSON list of `{ id, catalogId?, label, command, args,
  env: [{ name, value?, secret }], agentName? }`. A catalog agent's id is
  its catalog key (`claude`, `agy`, …; adding it again updates it); custom
  agents are `custom-1`, `custom-2`, ….
- `ai.agent.default`: `builtin` or an agent id.
- Secret-looking environment values (`KEY`, `TOKEN`, `SECRET`, `PASSWORD`,
  `PASSWD`, `CREDENTIAL`, `AUTH`) are sealed in the vault as
  `ai:acp-env:<agent id>:<NAME>` and never sent back to the window.
  Removing a variable or **removing the agent deletes its secrets**.
- The single agent of the earlier version (`ai.agent.kind`,
  `ai.acp.command`, `ai.acp.args`, `ai.acp.env`, `ai.acp.agent_name`) is
  moved once into the list as the agent `custom` (default if `kind` was
  `acp`), and the old keys are deleted. Its secrets keep their old vault
  names (`ai:acp-env:<NAME>`), so the move never opens the vault.

## MCP transport

ACP passes MCP servers in `session/new`. `stdio` is the only transport
every agent must support; `http` and `sse` are optional capabilities
(`agentCapabilities.mcpCapabilities`). Mystique's MCP server is a local
Streamable HTTP server with a bearer token. Per turn, after `initialize`:

1. the agent advertises `mcpCapabilities.http` → Mystique passes
   `{ type: "http", name: "mystique", url, headers: [Authorization: Bearer <token>] }`;
2. else, the app can run the bridge → Mystique passes a **stdio** server:
   `{ name: "mystique", command: <the Mystique binary>, args:
   ["--mcp-stdio-bridge"], env: [MYSTIQUE_MCP_URL, MYSTIQUE_MCP_TOKEN] }`;
3. else the turn is refused with the reason (Test shows it too).

Exactly one MCP server is passed either way: Mystique's.

### The stdio bridge

`crates/mystique-acp/src/bridge.rs`, entered from the first lines of
`src-tauri/src/main.rs` (before Tauri or any window exists) when the first
argument is `--mcp-stdio-bridge`:

- Reads newline-delimited JSON-RPC (MCP's stdio framing) from stdin, POSTs
  each message to the MCP URL with `Authorization: Bearer <token>`,
  `Accept: application/json, text/event-stream`, and writes each reply as
  one compact JSON line to stdout (plain JSON or the `data:` lines of an
  event stream). Notifications (HTTP 202) get no reply. The
  `Mcp-Session-Id` the server returns is sent back on later requests.
- Each message is posted on its own thread, so a tool call waiting in the
  approval dock does not hold up the rest.
- Errors become JSON-RPC errors for the request: server unreachable
  (`-32603`), token refused (`-32001`), other HTTP status (`-32603` unless
  the body is a JSON-RPC error); unparsable input gets `-32700`.
- **Exits when stdin closes** (after up to 2 s for replies in flight).
- Std only (no async runtime, no TLS, no new dependency).

Security:

- The URL and token come only from the environment of the bridge process
  (`MYSTIQUE_MCP_URL`, `MYSTIQUE_MCP_TOKEN`), set on the `mcpServers` stdio
  entry — **never on the command line** (`ps` shows only
  `--mcp-stdio-bridge`) and **never logged**: the bridge's stderr lines never
  format the token and mask it if it appears.
- Only a loopback `http://127.0.0.1|localhost|[::1]:<port>/…` URL without a
  query, fragment or user is accepted, so the token cannot be sent off the
  machine or end up in a URL.
- The bridge is a plain MCP client of the server: the token check, MCP
  access levels and the approval dock apply exactly as over HTTP.

## What Mystique offers and declines

| Capability | Mystique's answer |
|---|---|
| `clientCapabilities.fs.readTextFile` / `writeTextFile` | `false`: declined |
| `clientCapabilities.terminal` | `false`: declined |
| `fs/*`, `terminal/*` requests anyway | error `-32601` |
| MCP servers in `session/new` | exactly one: Mystique's (HTTP + bearer header, or the stdio bridge + env) |
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

1. Spawn the chat's agent (no shell) in its own process group, `cwd` = the
   empty folder, stdin/stdout piped for ACP, stderr captured.
2. `initialize` (90 s limit). Refuse protocol ≠ 1; pick the MCP transport
   (HTTP, else the bridge, else refuse).
3. `session/new` with Mystique's MCP server (90 s limit).
4. `session/prompt`; serve `session/update` and `session/request_permission`
   until the reply (10 min limit for the whole turn: a write may wait 90 s in
   the dock, and agents plan before calling tools).
5. Close stdin, give the agent 300 ms to exit, kill the process group.

**Stop** (and the time limit) sends `session/cancel`, keeps serving the agent
for 500 ms so it can end the turn, then kills the whole process group
(`npx` → node → the CLI; a bridge the agent started is in that group too and
also exits when its stdin closes). Stop returns within about a second even if
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
draft) and are tagged `provider: "acp"`, `model: <agent id>`. The agent's
own name from `initialize` is remembered on the agent, for the menu.

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

What this does not cover: tools the agent runs *without* asking. An agent
may read files in its working directory without a prompt — that folder is
empty. Agents also load MCP servers from their own settings (Claude Code's,
Cursor's `.cursor/mcp.json`, …); their calls are rejected when the agent
asks, but servers you set to auto-approve in that agent are outside
Mystique's control.

## Security model

- **The agent reaches databases only through Mystique's MCP server**, so it
  is an MCP caller: **MCP's access levels** (Settings → MCP → Access) apply,
  and its writes wait in the **approval dock** like any MCP client's (caller
  `mcp`). ACP is not a bypass: `a_write_waits_in_the_dock_and_is_denied_on_timeout`
  runs a write end to end, sees it in the dock and denied at the timeout,
  and checks nothing ran. Over the stdio bridge it is the same server, the
  same token check, the same levels and dock.
- The MCP server must be **running**; the turn is refused otherwise. The
  token is the existing MCP token, sent only in the `Authorization` header
  of the HTTP entry, or in the bridge's environment — never on a command
  line, never in a log line. The Test button's session carries no MCP
  server and so no token.
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
- **Detection runs programs** — the user's login shell (to read `PATH`),
  `npm prefix -g`, and `<cli> --version` of binaries it found — each with
  stdin closed, in its own process group, under a hard time limit, killed
  whole on timeout. The shell's output is only used as a list of folders.
  It never runs `npx` and never installs anything.
- The agent process runs as you, with your environment plus what you set.
  It is your agent: Mystique limits what *Mystique* gives it, not what the
  program can do on its own.

## Tests

`cargo test -p mystique-acp` (no real agent needed; a fake ACP agent speaks
over in-memory pipes against the real MCP server; process tests use
`/bin/sh`; detection uses a fake home and stub executables):

- catalog: `ids_are_unique_and_every_entry_is_complete`,
  `the_user_s_examples_are_listed`
- detection: `native_agents_adapters_and_missing_ones_are_told_apart`
  (PATH, newest nvm, `~/.local/bin` adapter, missing),
  `a_slow_version_is_skipped_and_a_non_executable_file_is_not_an_agent`,
  `the_login_shell_path_is_read_between_markers_and_a_hang_times_out`,
  `the_login_shell_path_finds_agents_the_app_path_misses`,
  `npm_global_bin_is_searched_when_npm_is_found`,
  `unconfirmed_agents_are_shown_but_never_launchable`,
  `search_dirs_keep_order_and_drop_duplicates`
- bridge: `lines_are_forwarded_with_the_bearer_token_and_replies_come_back_one_per_line`,
  `the_session_id_is_sent_back_after_the_server_assigns_one`,
  `event_stream_replies_and_errors_are_framed_as_json_rpc`,
  `an_unreachable_server_fails_requests_without_logging_the_token`,
  `only_loopback_urls_without_a_query_are_accepted`,
  `it_returns_when_stdin_closes`,
  `over_the_bridge_the_token_is_in_the_environment_not_the_arguments`,
  `http_wins_then_the_bridge_then_a_refusal`, and end to end
  `an_agent_without_http_mcp_reaches_mystique_through_the_stdio_bridge`
  (a stdio-only agent gets the bridge entry, the bridge runs `initialize`
  and a real `list_objects` against the real MCP server)
- settings: `agents_are_added_listed_changed_and_removed_with_their_secrets`,
  `the_default_and_a_per_chat_pick_choose_the_agent`,
  `the_single_agent_of_earlier_versions_moves_into_the_list`,
  `secret_looking_values_go_to_the_vault_and_never_back_to_the_window`,
  `bad_input_is_refused`, `a_missing_command_says_how_to_fix_it`
- turns: `the_test_button_initializes_and_declines_files_and_terminal`,
  `the_test_button_reports_sign_in_and_how_mcp_is_reached`,
  `a_turn_streams_text_and_a_tool_call_through_mystique_mcp`,
  `permission_is_allowed_once_for_mystique_tools_and_rejected_for_the_rest`,
  `stop_mid_turn_cancels_the_session_and_ends_within_a_second`,
  `stop_kills_the_agent_process_group`, `app_quit_kills_running_agents`,
  `a_write_waits_in_the_dock_and_is_denied_on_timeout`,
  `planted_secrets_never_reach_the_stream_logs_or_saved_rows`,
  `the_agent_s_stderr_is_logged_masked`,
  `an_agent_that_exits_or_lacks_http_mcp_fails_the_turn_with_a_reason`

Frontend (`scripts/workspace/acp-settings.test.ts`): menu choices, default
and per-chat choice (incl. removed agents and older chats), detected groups
and status labels, the store's add/remove bookkeeping, and the settings
section mounted against a fake backend (built-in, detected with Add,
unconfirmed without Add, not-installed collapsed with the install command,
Add → Test inline with the sign-in problem, Custom agent… form).

## Manual test

Against a local build (`pnpm tauri:dev`):

1. Settings → MCP: turn the server on. Access: set a test connection to
   `write`, and turn on "ask me for high-risk writes" for it.
2. Settings → AI → Agent: the "Detected on this Mac" list shows the agents
   you have (e.g. Claude Code via npx, Antigravity, Grok Build). Press
   **Add** on one: it is added (and made default if first) and tested
   inline. A signed-out agent says how to sign in.
3. Open a query tab and the agent pane. The header shows `Agent · <menu>`;
   pick the agent. Ask "What tables are there?". Text streams; a tool card
   ("Listing objects") appears and finishes.
4. With Antigravity (`agy acp`, stdio only): the same works through the
   bridge; `ps` shows `mystique --mcp-stdio-bridge` with no token in its
   arguments while the turn runs, and nothing left after it.
5. Switch the chat to Built-in, then to another chat and back: each chat
   keeps its agent. A new chat starts on the default.
6. Ask "Insert a test row into <table>". The `execute_query` card shows
   "waiting"; the approval dock shows the statement for MCP. Deny it: the
   card fails with the denial, nothing was written.
7. Press **Stop** while a tool call runs: the run ends within about a second;
   no agent process is left. Quit Mystique during a turn: none left either.
8. Remove the agent: it disappears from the pane's menu; its secrets are
   gone from the vault.

## Deferred

- Running an agent's sign-in flow (`authenticate`, terminal auth) from
  Mystique; Test only reports it.
- Installing agents (never automatic; could offer a "run in Terminal"
  button for the install command).
- Fetching the ACP registry at run time to update the catalog; it is a
  table in the source, updated by hand.
- Long-lived sessions per chat (`session/load` / keeping the process):
  each turn starts a fresh process and replays the chat as a transcript.
- Images and resources in prompts; diffs and terminals in tool content.
- Session modes and config options (the agent's own model picker).
- Token usage (`usage_update`, `unstable_end_turn_token_usage`).
- Agent-level (not MCP-level) access levels for the external agent: it would
  need the MCP server to know the caller is the in-app pane.
- Per-agent icons (the list uses two-letter tiles).
