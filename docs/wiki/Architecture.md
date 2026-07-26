# Architecture (summary)

MystiqueDB is a **Tauri 2** desktop app:

- **Frontend:** Svelte 5 / SvelteKit  
- **Backend:** Rust workspace (`mystique-core`, connector API, engine plugins)  
- **IPC:** thin Tauri commands → application services  

## Layers

```text
Svelte UI
  → Tauri IPC
    → mystique-core (profiles, sessions, query orchestration)
      → connector plugins (postgres / mysql / mongodb / …)
```

## Plugin SPI

- `DatabaseConnector` + `ConnectorSession` — connect, pool, engine operations  
- `EngineQueryPolicy` — **per-engine** validate-before-execute  
- Core does not import drivers; plugins register in a registry  

## Local storage

App metadata (profiles, saved queries, history, settings, ER diagrams) lives in a **local SQLite** database under the app data directory. Engine servers (Postgres, Mongo, MySQL) are external.

## Full architecture docs (repo)

- [`docs/ARCHITECTURE_AND_FEATURES.md`](../ARCHITECTURE_AND_FEATURES.md)  
- [`docs/BACKEND_STRUCTURE.md`](../BACKEND_STRUCTURE.md)  
- [`docs/SQLITE_SCHEMA.md`](../SQLITE_SCHEMA.md)  
