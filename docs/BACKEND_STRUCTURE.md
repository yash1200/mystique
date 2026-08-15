# Backend structure (Rust)

> Scalable layout for MystiqueDB desktop backend (API-first connectors).

## Workspace

```text
mystique-db/
├── Cargo.toml
├── crates/
│   ├── mystique-connector-api/     # SPI: connector + EngineQueryPolicy + DTOs (no drivers)
│   ├── mystique-core/              # domain + storage + ConnectionManager + AgentTools
│   │   └── src/connectors/         # registry (connectors + policies) + session/pool manager
│   ├── mystique-mcp/               # localhost MCP HTTP server (calls AgentTools)
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
| `mystique-connector-api` | serde, async-trait | engine drivers, tauri |
| `plugins/*` | connector-api + one driver | tauri, mystique-core |
| `mystique-core` | connector-api + plugins (registry only) | dialect-specific SQL/MQL parsing, HTTP/MCP |
| `mystique-mcp` | mystique-core (AgentTools) + axum | tauri, engine drivers |
| `src-tauri/ipc` | mystique-core, mystique-mcp, tauri | SQL, driver imports |

## Query policy SPI

- Trait + shared request/result types live in **`mystique-connector-api`**.
- **One `EngineQueryPolicy` implementation per engine plugin** (Postgres ≠ MySQL ≠ Mongo).
- Core `QueryService`: resolve policy by session `engine_id` → `validate` → execute.
- Modes: **Read** (default) · **Write** (opt-in) · **DELETE always denied** for now.
- Limits (max rows, timeout): core hard caps applied on every request.

## Adding a new database

1. Create `crates/plugins/mystique-plugin-<engine>` implementing `DatabaseConnector` + `ConnectorSession`.
2. Implement **`EngineQueryPolicy` for that engine** (validation before any execute).
3. Own pooling **inside** the session (`connect` builds the pool).
4. Register connector **and** policy in `ConnectorRegistry::bootstrap()` — two lines, not a core rewrite.
5. Expose via existing IPC (`list_engines`, `open_engine_session`, query commands, …).

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
