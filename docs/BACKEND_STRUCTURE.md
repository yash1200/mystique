# Backend structure (Rust)

> Scalable layout for MystiqueDB desktop backend (API-first connectors).

## Workspace

```text
mystique-db/
├── Cargo.toml
├── crates/
│   ├── mystique-connector-api/     # SPI: traits, DTOs, pool status (no drivers)
│   ├── mystique-core/              # domain + storage + ConnectionManager
│   │   └── src/connectors/         # registry + session/pool manager
│   └── plugins/
│       ├── mystique-plugin-postgres/   # deadpool + tokio-postgres
│       ├── mystique-plugin-sqlite/     # r2d2 + rusqlite
│       └── mystique-plugin-mongodb/    # mongodb client pool
└── src-tauri/                      # thin IPC → services
```

## Layer rules

| Layer | May depend on | Must not |
| ----- | ------------- | -------- |
| `mystique-connector-api` | serde, async-trait | engine drivers, tauri |
| `plugins/*` | connector-api + one driver | tauri, mystique-core |
| `mystique-core` | connector-api + plugins (registry only) | tauri UI types |
| `src-tauri/ipc` | mystique-core, tauri | SQL, driver imports |

## Adding a new database

1. Create `crates/plugins/mystique-plugin-<engine>` implementing `DatabaseConnector` + `ConnectorSession`.
2. Own pooling **inside** the session (`connect` builds the pool).
3. Register in `ConnectorRegistry::bootstrap()` — **one line**.
4. Expose via existing IPC (`list_engines`, `open_engine_session`, …) — **no new core APIs**.

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
