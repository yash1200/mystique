# Backend structure (Rust)

> Scalable layout for Mystique desktop backend (API-first connectors).

## Workspace

```text
mystique/
├── Cargo.toml
├── crates/
│   ├── mystique-sql/               # statement split + AST write-risk + ident quoting
│   ├── mystique-connector-api/     # SPI: connector + EngineQueryPolicy + DTOs (no drivers)
│   ├── mystique-core/              # domain + storage + ConnectionManager + AgentTools
│   │   └── src/connectors/         # registry (connectors + policies) + session/pool manager
│   ├── mystique-mcp/               # localhost MCP HTTP server (calls AgentTools)
│   ├── mystique-agent/             # in-app query agent (Rig + AgentTools)
│   └── plugins/
│       ├── mystique-plugin-postgres/   # pool + session + PostgresQueryPolicy
│       ├── mystique-plugin-mysql/      # pool + session + SqlQueryPolicy("mysql")
│       ├── mystique-plugin-mongodb/    # pool + session + MongoQueryPolicy
│       └── mystique-plugin-sqlite/     # later / local files
└── src-tauri/                      # thin IPC → services
```

## Layer rules

| Layer | May depend on | Must not |
| ----- | ------------- | -------- |
| `mystique-sql` | sqlparser | connector-api, drivers, tauri |
| `mystique-connector-api` | serde, async-trait, mystique-sql | engine drivers, tauri |
| `plugins/*` | connector-api + one driver | tauri, mystique-core |
| `mystique-core` | connector-api + plugins (registry only) | dialect-specific SQL/MQL parsing, HTTP/MCP |
| `mystique-mcp` | mystique-core (AgentTools) + axum | tauri, engine drivers |
| `mystique-agent` | mystique-core (AgentTools, chats) + Rig | tauri, engine drivers, MCP HTTP |
| `src-tauri/ipc` | mystique-core, mystique-mcp, mystique-agent, tauri | SQL, driver imports |

## Query policy SPI

- Trait + shared request/result types live in **`mystique-connector-api`**.
- **One `EngineQueryPolicy` implementation per engine plugin** (Postgres ≠ MySQL ≠ Mongo).
- Core `QueryService`: resolve policy by session `engine_id` → `validate` → execute.
- Modes: **Read** (default) · **Write** (opt-in). DELETE is a write (allowed in Write mode, blocked in Read). DROP/TRUNCATE are Destructive (P3.1 type-to-confirm). SQL risk comes from `mystique-sql` (AST), not a first-keyword list.
- Limits (max rows, timeout): core hard caps applied on every request.
- The per-profile query timeout (0 = none) reaches the server on every engine: `statement_timeout` (Postgres), `max_execution_time` (MySQL), and `maxTimeMS` on every Mongo command, writes included. A Mongo `maxTimeMS` option can lower it, not raise it.

### Mongo shell text

- `mystique-plugin-mongodb/src/shell` is the one parser for the executor, the write policy, the guard preview and explain.
- Options arguments are applied, never parsed and dropped: `find(filter, projection, options)`, `aggregate(pipeline, options)`, `updateOne(filter, update, { upsert, arrayFilters, … })`, `findOneAndUpdate(…, { returnDocument, projection, sort, … })`, `insertMany(docs, { ordered })`, `bulkWrite(ops, { ordered })`, and per-operation options inside `bulkWrite`. An option a method does not take is refused with the list it does take. `shell/options.rs` has the table.
- Options never lower risk: `upsert` is still a write; `$out` / `$merge` anywhere keep an aggregate a write.
- insert/update/delete run as write commands (`session/write.rs`) so they carry `maxTimeMS`. A failed `insertMany` / `bulkWrite` names each write error with its index and the counts applied before it (outside a transaction). These commands are not retried on a transient network error.
- A `find` without an explicit limit counts its matches alongside the page (not after it) and shows `(N matching)` when the page is truncated. The table grid starts its total count with the page.

## Adding a new database

1. Create `crates/plugins/mystique-plugin-<engine>` implementing `DatabaseConnector` + `ConnectorSession`.
2. Write `default_capabilities()` in the plugin (`src/capabilities.rs`). Start from `EngineCapabilities::relational(engine, dialect)` or `EngineCapabilities::document(engine, language)` and override what differs. Nothing in `mystique-connector-api` names your engine.
3. Declare every catalog kind you list in `object_kinds` (`ObjectKindSpec`): `browsable` opens a grid, `editable` allows grid edits and import, `primary` lists it with tables instead of under its own heading. The explorer renders from the spec and never matches on the id. Rename, drop, truncate and duplicate apply only to `table`, `view` and `collection`; any other kind offers its definition (`TableAdminOp::CreateStatement`, with `TableAdminInput::object_kind`).
4. Implement **`EngineQueryPolicy` for that engine** (validation before any execute).
5. Own pooling **inside** the session (`connect` builds the pool).
6. Pick a paging style. `PagingStyle::Offset` (default) pages by `offset`. `PagingStyle::Token` engines (Redis `SCAN`, Elasticsearch `search_after`) read `FetchTablePreviewInput::page_token` and return `next_page_token`; the default `stream_rows` follows the tokens.
7. A catalog too large to list at once (a keyspace, a cluster) overrides `list_catalog_children` (one level, paged, with a server-side name filter) and sets `supports_catalog_paging`. The default derives pages from `list_catalog`.
8. A result that is not a table returns `QueryResult::from_payload(…)` with a `QueryPayload` (`Value`, `Hits`, `Message`). The tabular form is filled in for export, history, MCP and the agent; `clip_query_result` keeps the payload under 1 MiB.
9. Implement **`fetch_server_metrics`** (and `terminate_backend_session` if the engine can kill sessions). Fill the shared snapshot; put engine-only facts in `overview.extras` / row `extras`. Set `supports_server_metrics` / `supports_terminate_session` on capabilities.
10. Register connector **and** policy in `ConnectorRegistry::bootstrap()` — two lines, not a core rewrite.
11. Expose via existing IPC (`list_engines`, `open_engine_session`, query commands, …).
12. **Pass the conformance suite** (`docs/CONFORMANCE.md`): add `tests/conformance.rs` with a `Fixture`, and a step to the `live` CI job.

## Connection pool API (IPC)

| Command | Purpose |
| ------- | ------- |
| `list_engines` | Catalog of registered plugins |
| `test_engine_connection` | One-shot connectivity check |
| `open_engine_session` | Create pooled session → `sessionId` |
| `close_engine_session` | Drain pool / shutdown |
| `ping_engine_session` | Health check |
| `engine_session_pool_status` | Active/idle/max snapshot |
| `list_engine_sessions` / `get_engine_session` | Session inventory |
| `fetch_engine_server_metrics` | Overview + live sessions + slow queries (plugin-owned) |
| `terminate_engine_session` | Kill a backend pid / opid (denied when the profile is read-only) |

Request body (`OpenSessionRequest`):

```json
{
  "engine": "postgres",
  "pluginConfig": { "host": "127.0.0.1", "port": 5432, "database": "app", "username": "postgres" },
  "password": "secret",
  "pool": { "maxSize": 10, "minIdle": 1, "acquireTimeoutMs": 10000, "idleTimeoutSecs": 300 }
}
```

## Call path (engine)

```text
Svelte invoke("open_engine_session", { request })
  → ipc::engines::open_engine_session
  → ConnectionManager::open_session
  → ConnectorRegistry.get(engine)
  → plugin.connect(config)  // builds pool
  → SessionInfo
```
