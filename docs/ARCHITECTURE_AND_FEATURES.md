# MystiqueDB — System Architecture & Feature Plan

> **Status:** Engineering architecture (implements the product PRD)  
> **Product PRD:** [`PRD.md`](./PRD.md) — product requirements, phases, acceptance  
> **Feature catalog:** [`FEATURES.md`](./FEATURES.md) — priority product features checklist  
> **UI/UX:** [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md) — product themes, connection colors, shell  
> **Design language (code):** [`DESIGN_LANGUAGE.md`](./DESIGN_LANGUAGE.md) — Tailwind tokens, type scale, forbidden patterns  
> **Local SQLite schema:** [`SQLITE_SCHEMA.md`](./SQLITE_SCHEMA.md) — profiles, queries, multi-engine `plugin_config`  
> **Backend layout:** [`BACKEND_STRUCTURE.md`](./BACKEND_STRUCTURE.md) — workspace, layers, extension points  
> **Kanban:** [`KANBAN.md`](./KANBAN.md) — TODO / Progress / QA / Deployed tickets (`FE` / `BE`)  
> **Stack (this repo):** Tauri 2 (Rust) · Svelte 5 / SvelteKit SPA  
> **Date:** 2026-07-25  
> **UI engines (picker):** PostgreSQL · MongoDB · SQLite · **Phase 1 live depth:** PostgreSQL first (plugins)

This document is the **engineering plan** for MystiqueDB. Product intent, scope, and success criteria come from the PRD; this file defines **how we build it**: plugin connectors, layers, DTOs, features by phase, and the current scaffold.

---

## 1. Purpose & relationship to the PRD

| Document | Owns |
| -------- | ---- |
| [`PRD.md`](./PRD.md) | Why, who, phased scope, functional requirements, Model view product rules |
| **This file** | System design, plugin SPI, code layout, tech mapping, feature engineering list |
| [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md) | Visual system, light/dark, connection color identity |

**Product one-liner (from PRD):**  
A multi-engine database studio: select engine → connect → explore structure, data, and **Model view**—with depth that matches how that database works. Shared shell, engine-honest UX, **connector-first architecture**.

**Mystique naming:** the shell adapts to the engine (relational ER vs document map vs key model) without a full UI rewrite. New engines are **plugins**, not forks of the app.

---

## 2. Architectural goals

1. **Plugin-first engines** — Adding MySQL, SQLite, MongoDB, etc. means a new connector package + optional UI form/icons, not rewiring Explorer/Data/Query/Model.
2. **Postgres-first depth** — Phase 1 ships an excellent PostgreSQL loop before multi-engine marketing.
3. **Capability-driven UI** — Backend returns capabilities; frontend shows/hides tabs and labels.
4. **FE presents, BE decides** — Drivers, policy, secrets, metadata, and model graphs live in Rust; Svelte only renders.
5. **Safety by default** — Phase 1 is **read-oriented** (SQL read-only policy, timeouts, max rows).
6. **Normalized contracts** — Core DTOs (`Namespace`, `Container`, `Field`, `Relationship`, `DataPage`, `ModelGraph`) stay stable across plugins.
7. **Lightweight desktop** — Tauri shell + in-process Rust core (single binary; no JVM sidecar in this repo).

---

## 3. High-level system

```
┌──────────────────────────────────────────────────────────────────┐
│  Tauri 2 desktop shell                                           │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │  Svelte 5 UI                                               │  │
│  │  Home (engine picker) · Connect · Workspace                │  │
│  │  Explorer · Data · Query · Model view                      │  │
│  │  Themes · connection colors · capability-driven chrome     │  │
│  └───────────────────────────┬────────────────────────────────┘  │
│                              │ invoke / events (typed IPC)       │
│  ┌───────────────────────────▼────────────────────────────────┐  │
│  │  Tauri commands (thin adapters)                            │  │
│  │  connection · metadata · data · query · model · export     │  │
│  └───────────────────────────┬────────────────────────────────┘  │
│  ┌───────────────────────────▼────────────────────────────────┐  │
│  │  mystique-core                                             │  │
│  │  ├── Session / ConnectionManager                           │  │
│  │  ├── QueryPolicy (read-only, timeout, max rows)            │  │
│  │  ├── MetadataService                                       │  │
│  │  ├── DataService                                           │  │
│  │  ├── ModelGraphService                                     │  │
│  │  ├── ExportService                                         │  │
│  │  ├── AuditLog                                              │  │
│  │  └── ConnectorRegistry  ──► plugins/*                      │  │
│  └───────────────────────────┬────────────────────────────────┘  │
└──────────────────────────────┼───────────────────────────────────┘
                               │
          ┌────────────────────┼────────────────────┐
          ▼                    ▼                    ▼
   plugin-postgres      plugin-sqlite (P2)   plugin-mongo (later)
          │
          ▼
     PostgreSQL server
```

**Principle:** Core services never import `tokio_postgres` (or any engine crate) directly. They only depend on the **Connector SPI**. Engine crates implement the SPI and register themselves.

---

## 4. Plugin architecture (connector SPI)

### 4.1 Why plugins

Without a SPI, each new database tends to:

- Branch the UI (`if (engine === 'mongo') …` everywhere)
- Duplicate connection/query/export code
- Break Model view when semantics differ

With plugins:

| Layer | Stable | Per-engine plugin |
| ----- | ------ | ----------------- |
| App shell, tabs, themes | ✓ | |
| Connection profiles store | ✓ | Form schema + validation hints |
| Explorer / Data / Query / Model chrome | ✓ | Optional cell/render helpers |
| Normalized DTOs + capabilities | ✓ | Mapping from native metadata |
| Drivers, SQL/MQL dialect, model graph builder | | ✓ |

**Acceptance (from PRD):** a second engine must use the **same SPI** as Postgres without changing core DTOs drastically.

### 4.2 Connector trait (SPI)

Conceptual Rust surface (names may match code closely):

```rust
/// Stable plugin contract. One implementation crate per engine family/engine.
#[async_trait]
pub trait DatabaseConnector: Send + Sync {
    /// Stable id: "postgres", "sqlite", "mongodb", …
    fn id(&self) -> &'static str;

    fn display_name(&self) -> &'static str;

    /// Family drives default Model view semantics and some chrome.
    fn family(&self) -> EngineFamily; // Relational | Document | WideColumn | KeyValue | Other

    /// Static capabilities (refined after connect if needed).
    fn capabilities(&self) -> EngineCapabilities;

    /// JSON-schema-like description for dynamic connection forms (UI).
    fn connection_form_schema(&self) -> ConnectionFormSchema;

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<TestResult, ConnectorError>;

    async fn connect(&self, config: &ConnectionConfig) -> Result<Box<dyn ConnectorSession>, ConnectorError>;
}

/// Live session for one user connection (pooled or single).
#[async_trait]
pub trait ConnectorSession: Send + Sync {
    fn capabilities(&self) -> EngineCapabilities;

    async fn list_namespaces(&self) -> Result<Vec<Namespace>, ConnectorError>;
    async fn list_containers(&self, ns: &NamespaceId) -> Result<Vec<Container>, ConnectorError>;
    async fn list_fields(&self, container: &ContainerRef) -> Result<Vec<Field>, ConnectorError>;
    async fn list_indexes(&self, container: &ContainerRef) -> Result<Vec<IndexInfo>, ConnectorError>;
    async fn list_relationships(&self, scope: &MetadataScope) -> Result<Vec<Relationship>, ConnectorError>;

    async fn fetch_data_page(&self, req: DataPageRequest) -> Result<DataPage, ConnectorError>;

    async fn execute_query(&self, req: QueryRequest) -> Result<QueryResult, ConnectorError>;
    async fn cancel_query(&self, query_id: &QueryId) -> Result<(), ConnectorError>;

    /// Build Model view graph (ER, collection map, key model, …).
    async fn build_model_graph(&self, opts: ModelBuildOptions) -> Result<ModelGraph, ConnectorError>;

    async fn export_query(&self, req: ExportRequest, sink: &mut dyn ExportSink) -> Result<ExportStats, ConnectorError>;

    async fn close(&self) -> Result<(), ConnectorError>;
}
```

### 4.3 What a plugin package contains

```
crates/plugins/mystique-plugin-postgres/
├── Cargo.toml                 # depends on mystique-connector-api + tokio-postgres
└── src/
    ├── lib.rs                 # register() → Box<dyn DatabaseConnector>
    ├── connector.rs           # DatabaseConnector impl
    ├── session.rs             # ConnectorSession impl
    ├── metadata.rs            # information_schema / pg_catalog mapping → DTOs
    ├── query.rs               # execute + read-only guard helpers (policy still core)
    ├── model.rs               # PostgresModelBuilder → ModelGraph (FK/PK only)
    └── form.rs                # host, port, db, user, ssl mode, …
```

Optional frontend companion (not a separate process):

```
src/lib/plugins/postgres/
├── icon.ts                    # engine icon component id
├── form.svelte                # or pure schema-driven form from connection_form_schema
└── labels.ts                  # "schema", "table", "view" copy overrides if needed
```

Most UI should stay **schema-driven** from `connection_form_schema` + `capabilities` so new engines need little or no Svelte.

### 4.4 Registry & discovery

```rust
pub struct ConnectorRegistry {
    connectors: HashMap<&'static str, Arc<dyn DatabaseConnector>>,
}

impl ConnectorRegistry {
    pub fn bootstrap() -> Self {
        let mut reg = Self::default();
        // Phase 1: only Postgres is registered and user-visible.
        reg.register(mystique_plugin_postgres::connector());
        // Later:
        // reg.register(mystique_plugin_sqlite::connector());
        // reg.register(mystique_plugin_mongodb::connector());
        reg
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn DatabaseConnector>>;
    pub fn list_visible(&self) -> Vec<ConnectorDescriptor>; // home engine picker
}
```

**Phase 1 rule:** only `postgres` appears on the Home engine picker. Other plugins may exist as stubs for compile-time SPI proof but stay **hidden** until ready (see engine “Supported” checklist in PRD §14).

### 4.5 Core vs plugin responsibilities

| Concern | Core (`mystique-core`) | Plugin |
| ------- | ---------------------- | ------ |
| Profile CRUD, encryption refs | ✓ | Provides form fields |
| Open session, track connection id | ✓ | `connect` / `close` |
| Read-only SQL policy, timeouts, max rows | ✓ (policy engine) | May supply dialect helpers |
| Query audit log | ✓ | — |
| Normalize errors for UI | ✓ maps `ConnectorError` | Returns structured errors |
| PK/FK discovery | — | ✓ (Postgres: catalogs) |
| Model graph layout | Optional shared layout utils | **Builds** graph nodes/edges |
| CSV/JSON export streaming | Orchestration + file sink | Produces rows |
| UI grid / ER canvas | Svelte | — |

### 4.6 Capabilities (drive the UI)

Returned after connect (and optionally refined):

```json
{
  "engine": "postgres",
  "family": "RELATIONAL",
  "queryLanguage": "SQL",
  "supportsSchema": true,
  "supportsForeignKeys": true,
  "supportsErDiagram": true,
  "supportsModelView": true,
  "modelViewKind": "ER",
  "supportsPagination": true,
  "supportsWrite": false,
  "supportsCancelQuery": true,
  "objectLabels": {
    "namespace": "Schema",
    "container": "Table",
    "field": "Column"
  }
}
```

UI rules:

- Hide features when flags are false.
- Rename Explorer labels via `objectLabels`.
- Model view renderer selects layout/edge styling by `modelViewKind` (`ER` | `COLLECTION_MAP` | `KEY_MODEL` | …).

### 4.7 Adding a new database (checklist)

1. Create `mystique-plugin-<engine>` implementing `DatabaseConnector` + `ConnectorSession`.
2. Map native metadata → shared DTOs (no new Explorer APIs).
3. Implement `build_model_graph` with **documented** edge semantics (`FK` confirmed vs `INFERRED`, etc.).
4. Register in `ConnectorRegistry`; gate visibility until PRD §14 checklist passes.
5. Add connection form schema (+ optional icon).
6. Integration tests (testcontainers where possible).
7. **Do not** edit core services for engine-specific SQL strings.

---

## 5. Shared domain model (stable DTOs)

Plugins map into these types; UI only consumes them.

| DTO | Meaning |
| --- | ------- |
| `Namespace` | Schema / database / keyspace (engine-defined) |
| `Container` | Table, view, collection, … (`kind` discriminant) |
| `Field` | Column / document field path + type + nullable |
| `Relationship` | Declared or inferred link between containers |
| `DataPage` | Columns + rows + page info + truncated flag |
| `QueryResult` | Same as page + timing + warnings |
| `ModelGraph` | Nodes + edges for Model view |
| `EngineCapabilities` | Feature flags for UI |
| `ConnectionConfig` | Opaque JSON bag validated by plugin + core envelope (id, name, engine, color) |

### 5.1 Model graph (PRD contract)

```text
ModelGraph {
  engine, modelViewKind, generatedAt
  nodes[]: { id, name, namespace, kind, fields[], badges[] }
  edges[]: { id, fromNodeId, toNodeId, kind, label, confidence, detail }
}

edge.kind:     FK | INFERRED | SHARED_KEY | MANUAL | ACCESS_PATH
confidence:    CONFIRMED | HIGH | MEDIUM | LOW
```

**Relational (Postgres) policy:** edges only from **declared** PK/FK metadata → `kind = FK`, `confidence = CONFIRMED`. **Never invent** relationships.

**Document (future):** inferred refs allowed only with `INFERRED` + confidence labels.

### 5.2 Connection profile envelope (core)

```text
ConnectionProfile {
  id, name, engineId,
  colorId,                 // UI identity — see UI_UX_GUIDELINES
  isProduction?,           // optional PROD badge
  readOnly: true,          // Phase 1 default
  secretRef,               // keychain / encrypted store pointer
  pluginConfig: { … }      // engine-specific fields (host, port, …)
}
```

---

## 6. Application layers

### 6.1 Frontend (Svelte)

| Area | Responsibility |
| ---- | -------------- |
| Home | Engine picker (from registry descriptors) |
| Connection forms | Render schema from plugin; test connection; save profile |
| Workspace shell | Explorer \| Data \| Query \| Model (capability-gated) |
| Stores | Active session, tabs, theme, connection colors |
| Model canvas | Svelte Flow (or equivalent) + layout (dagre/ELK) |
| Grid / editor | Virtualized grid; CodeMirror SQL |

No database drivers in the frontend.

### 6.2 Tauri command layer

Thin IPC:

- Validate input, call core services, map errors to JSON.
- No business logic beyond authz of local file paths for export.

### 6.3 Core services

| Service | Role |
| ------- | ---- |
| `ConnectionService` | Profiles, secrets, open/close sessions via registry |
| `MetadataService` | Namespaces → containers → fields (cached, refreshable) |
| `DataService` | Paginated preview with server-side caps |
| `QueryService` | Policy → plugin execute; audit; cancel |
| `ModelService` | Calls `build_model_graph`; optional layout assist |
| `ExportService` | Stream results to CSV/JSON on disk |
| `AuditService` | Local log: time, engine, query text, success, row count |

### 6.4 Query policy (Phase 1)

Enforced in **core** before/around plugin execution for SQL families:

| Rule | Phase 1 |
| ---- | ------- |
| Default connection mode | Read-only |
| Allowed statements | Single `SELECT` (and optionally `WITH`…`SELECT`, `EXPLAIN` later) |
| Multi-statement | Rejected |
| Max rows | Configurable + hard cap |
| Timeout | Statement/query timeout |
| Writes | Not available in UI |

Plugins may expose dialect detection helpers; **policy remains centralized** so every SQL plugin stays safe by default.

---

## 7. Features list (engineering)

Aligned with PRD functional requirements; updated for **Phase 1 = PostgreSQL**.

### 7.1 Phase 1 — PostgreSQL studio + plugin foundation

**Platform & shell**

| Feature | Priority | Notes |
| ------- | -------- | ----- |
| Desktop app (Tauri) | P0 | Current repo |
| Home: engine picker | P0 | Only Postgres visible |
| Connection profiles save/edit/delete | P0 | Named profiles |
| Secure secret storage | P0 | OS keychain / encrypted store; no plaintext secrets on disk |
| Test connection | P0 | Human-readable errors (C-7) |
| Workspace: Explorer · Data · Query · Model | P0 | Single consistent layout |
| Dark / light / system theme | P0 | Per UI guidelines |
| Connection colors | P0 | Per UI guidelines |
| Multi-connection tabs | P1 | Unlimited; color-coded |
| Multi-object / query tabs | P0 | Tables + queries |

**PostgreSQL plugin (full depth)**

| Feature | Priority | Notes |
| ------- | -------- | ----- |
| Connect (host/port/db/user/password/SSL modes) | P0 | |
| Explorer: databases/schemas, tables, views | P0 | Views badged (M-4) |
| Columns + types on expand | P0 | |
| Indexes in detail panel | P1 | |
| Refresh metadata | P0 | |
| Data preview pagination | P0 | Server-side max cap |
| Sort where safe | P1 | |
| NULL / type-aware rendering | P1 | json/jsonb, bool, dates |
| Copy cell / row | P1 | |
| SQL editor + run | P0 | Timing + errors |
| Read-only SQL policy | P0 | Block non-SELECT; no multi-statement |
| Max rows + timeout | P0 | Core policy |
| Export results CSV / JSON | P0 | Stream from Rust; not full dump via WebView |
| Model view **ER** from live PK/FK | P0 | No invented edges |
| Node click → open Data | P0 | |
| Edge click → FK detail | P0 | |
| Model search / filter / readable layout (≤ ~80 tables) | P0 | |
| Query audit log (local) | P1 | |
| Cancel in-flight query | P2 | If driver supports |

**Architecture deliverables (must ship in Phase 1)**

| Deliverable | Priority |
| ----------- | -------- |
| `DatabaseConnector` / `ConnectorSession` SPI | P0 |
| `ConnectorRegistry` | P0 |
| Normalized DTOs + `ModelGraph` | P0 |
| `mystique-plugin-postgres` as first implementation | P0 |
| Capability flags end-to-end | P0 |
| Stub or hidden second plugin slot (optional compile test) | P1 |

**Phase 1 explicit non-goals**

- MongoDB / Cassandra / Redis / DynamoDB (product UI)
- MySQL/SQLite **shipping** as supported engines (SQLite is Phase 2)
- Inline row edit / writes
- Migrations / DDL design apply
- CSV **import**
- SSH tunnel, IAM auth
- Team cloud sync, collaboration
- AI assist, MCP
- Column View (may return later; not PRD V1 center)
- Mobile

### 7.2 Phase 2 — Second connector + relational expansion

| Feature | Priority | Plugin |
| ------- | -------- | ------ |
| **SQLite** full relational subset | P0 | `mystique-plugin-sqlite` — proves SPI |
| MySQL / MariaDB | P1 | `mystique-plugin-mysql` |
| Query history + saved queries (local) | P1 | Core |
| Postgres EXPLAIN visualization | P2 | Postgres plugin + UI |
| SSH tunnel | P1 | Core transport helper used by plugins |
| Model view UX polish (export PNG/SVG, collapse schema) | P1 | Core UI |
| FK cell navigation to related rows | P2 | Relational plugins |

### 7.3 Phase 3 — Second family (document)

| Feature | Priority | Plugin |
| ------- | -------- | ------ |
| MongoDB connect + databases/collections | P0 | `mystique-plugin-mongodb` |
| Document preview | P0 | |
| Find / read queries (MQL) | P0 | |
| Inferred schema / field map | P0 | |
| Model view = collection map + **inferred** refs (labeled) | P0 | |
| Pretty JSON view | P1 | UI |
| Optional gated writes (later) | P2 | Core policy + plugin |

### 7.4 Later

Cassandra/Keyspaces, DynamoDB, Redis, warehouses, schema diff, CSV import, web client against same core API, AI assist (never auto-run destructive SQL), ER design → DDL.

---

## 8. Phase 1 user flow (Postgres)

```
Open MystiqueDB
  → Home: select PostgreSQL
  → Connection form (plugin schema) + color
  → Test connection → Save profile → Connect
  → Workspace
       ├── Explorer  (schemas → tables/views → columns)
       ├── Data      (paginated grid)
       ├── Query     (SQL, read-only policy)
       └── Model     (ER from PK/FK; click-through to Data)
```

Success metrics (PRD):

| Metric | Target |
| ------ | ------ |
| Time to useful schema after install | < 5 minutes on local Postgres |
| Model view correctness | All declared FKs; no invented edges |
| Stability | No crash on happy path + common errors |
| Query safety | Non-SELECT rejected in read-only mode |
| Extensibility | SPI ready for SQLite without core rewrite |

---

## 9. Target code structure

```
mystique-db/
├── docs/
│   ├── PRD.md
│   ├── ARCHITECTURE_AND_FEATURES.md    # this file
│   └── UI_UX_GUIDELINES.md
├── src/                                # SvelteKit SPA
│   ├── lib/
│   │   ├── api/                        # invoke wrappers
│   │   ├── stores/
│   │   ├── components/
│   │   │   ├── layout/                 # AppShell, tabs, sidebar
│   │   │   ├── connection/
│   │   │   ├── explorer/
│   │   │   ├── data/
│   │   │   ├── query/
│   │   │   ├── model/                  # ER canvas
│   │   │   └── ui/                     # design-system primitives
│   │   ├── plugins/                    # optional engine UI extras
│   │   │   └── postgres/
│   │   ├── styles/tokens.css
│   │   └── types/                      # mirror core DTOs
│   └── routes/
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs                      # manage state, register commands
│   │   ├── commands/                   # thin IPC
│   │   └── state.rs
│   ├── Cargo.toml                      # depends on mystique-core
│   └── tauri.conf.json
└── crates/
    ├── mystique-connector-api/         # traits + DTOs + errors ONLY
    │   └── src/{lib,connector,dto,error,capabilities}.rs
    ├── mystique-core/                  # services, registry, policy, audit
    │   └── src/{lib,connection,metadata,data,query,model,export,audit,policy}.rs
    └── plugins/
        ├── mystique-plugin-postgres/   # Phase 1
        ├── mystique-plugin-sqlite/     # Phase 2
        └── mystique-plugin-mongodb/    # Phase 3
```

**Dependency rule:**

```
plugins/*  →  mystique-connector-api
mystique-core  →  mystique-connector-api  (+ loads plugins)
src-tauri  →  mystique-core
Svelte  →  IPC only (no plugin crates)
```

Plugins must **not** depend on `mystique-core` (avoids cycles). Core depends on the API crate and plugin crates as registry backends.

---

## 10. IPC command surface (Phase 1)

```
# Catalog
list_engines

# Profiles
list_profiles, save_profile, delete_profile, get_profile

# Session
test_connection, connect, disconnect, get_capabilities

# Metadata
list_namespaces, list_containers, list_fields, list_indexes, refresh_metadata

# Data
fetch_data_page

# Query
execute_query, cancel_query

# Model
get_model_graph

# Export
export_result

# App
get_settings, update_settings
```

All paths take `connection_id` / `session_id` after connect. UI never passes raw passwords after save—only profile ids (secrets resolved in core).

---

## 11. Current repository state

```
mystique-db/
├── src/routes/+page.svelte     # Tauri greet scaffold only
├── src-tauri/src/lib.rs        # greet command only
├── docs/PRD.md
├── docs/UI_UX_GUIDELINES.md
└── docs/ARCHITECTURE_AND_FEATURES.md
```

**Not yet built:** connector SPI, Postgres plugin, workspace UI, Model view, policy, export.

---

## 12. Implementation roadmap

| Milestone | Outcome |
| --------- | ------- |
| **M0 — Foundations** | Cargo workspace; `mystique-connector-api`; `mystique-core` skeleton; registry; Svelte app shell + tokens/theme; mock connector for UI |
| **M1 — Postgres connect** | `mystique-plugin-postgres`; profiles + secrets; test/connect; Explorer metadata |
| **M2 — Data + Query** | Paginated data; SQL editor; read-only policy; max rows/timeout; errors |
| **M3 — Model view** | ER graph from PK/FK; navigation; search/filter |
| **M4 — Phase 1 harden** | Export CSV/JSON; audit log; connection colors; packaging; docs |
| **M5 — SPI proof** | SQLite plugin (Phase 2 start) using same SPI |
| **M6 — Document family** | Mongo plugin + collection Model view |

---

## 13. Non-functional requirements (engineering)

| Area | Target |
| ---- | ------ |
| Performance | ≤ 200-table Postgres metadata &lt; 5s; simple data page &lt; 2s |
| Safety | Read-only default; policy in core |
| Security | Secrets not in renderer logs; encrypted at rest |
| Reliability | Plugin errors isolated; session failure ≠ app crash |
| Extensibility | New engine = new plugin crate + register + form schema |
| Size | Prefer lean release profile (LTO/strip already in Tauri `Cargo.toml`) |

---

## 14. Competitive context (brief)

| Tool | Strength | Gap MystiqueDB exploits |
| ---- | -------- | ----------------------- |
| DBeaver | Breadth | Heavy UX; weaker modern Model-first flow |
| DataGrip | SQL IDE depth | Paid; multi-family model story secondary |
| Beekeeper / TablePlus | Polished SQL UX | Engine/model depth gated or narrow |
| Compass | Mongo depth | Single family |
| DBX | Tauri multi-engine | Different product bet; we prioritize Model view + plugin honesty + Postgres depth first |

**Positioning:** Capability-honest multi-engine studio with **Model view as structural center** and **plugins for every engine**—Phase 1 proves it on PostgreSQL.

---

## 15. Risks & mitigations

| Risk | Mitigation |
| ---- | ---------- |
| Scope explosion | Phase 1 = Postgres only in UI; SPI ready, not 10 engines |
| Plugin API thrash | Keep `mystique-connector-api` tiny; version DTOs carefully |
| Fake ER edges | CONFIRMED FK only for relational |
| Huge schemas | Search, schema filter, focus neighbors |
| Unsafe queries | Core read-only policy + timeout + max rows |
| UI engine special cases | Capabilities + form schema; resist one-off branches |

---

## 16. Open decisions

From PRD, resolved for **this repo** where possible:

| Topic | Decision for MystiqueDB |
| ----- | ----------------------- |
| Desktop shell | **Tauri 2** (existing scaffold) |
| Backend runtime | **Rust in-process** (not Java sidecar)—same SPI idea as PRD |
| Phase 1 engine | **PostgreSQL only** |
| Phase 2 proof engine | **SQLite** (same relational module/SPI) |
| Mongo | Phase 3 (plugin), not Phase 1 |
| Product name | MystiqueDB (repo); PRD working title UDS maps to same product |
| Licensing | MIT in `package.json` today—confirm before public release |

Still open: OS install priority (macOS/Windows/Linux), exact secret store crate, diagram library final choice (Svelte Flow recommended).

---

## 17. Summary

| Topic | Decision |
| ----- | -------- |
| Source of product truth | [`PRD.md`](./PRD.md) |
| Architecture style | **Plugin connectors** (`DatabaseConnector` SPI) + capability-driven UI |
| Phase 1 | PostgreSQL full loop: connect → explore → data → query → **ER Model view** |
| Phase 1 features | Profiles, secure secrets, read-only SQL, export CSV/JSON, themes, connection colors |
| Non-goals Phase 1 | Writes, multi-engine marketing, Mongo UI, import, SSH |
| Adding a DB later | New plugin crate + register—**no core rewire** |
| UI system | [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md) |

---

## 18. Document history

| Version | Date | Notes |
| ------- | ---- | ----- |
| 1.0 | 2026-07-25 | Initial competitor-driven Tauri plan |
| 2.0 | 2026-07-25 | Rewritten from PRD: Postgres-first, plugin SPI, updated features, Model view center |

---

*Core stays engine-agnostic. Plugins make MystiqueDB transform—starting with PostgreSQL.*
