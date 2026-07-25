# Product Requirements Document (PRD)

# MystiqueDB (Universal Database Studio)

| Field                      | Value                                                           |
| -------------------------- | --------------------------------------------------------------- |
| **Product name**           | **MystiqueDB** (working subtitle: Universal Database Studio)    |
| **Document type**          | Product Requirements Document                                   |
| **Version**                | 1.2                                                             |
| **Status**                 | Draft for alignment · implemented in this repo                  |
| **Last updated**           | 2026-07-25                                                      |
| **Primary platforms**      | Desktop application (primary), Web client (future-friendly API) |
| **Backend**                | **Rust** + Tauri 2                                              |
| **Frontend**               | **Svelte 5** / SvelteKit                                        |
| **Kanban**                 | [`KANBAN.md`](./KANBAN.md) — TODO / Progress / QA / Deployed    |
| **UI contract**            | [`DESIGN_LANGUAGE.md`](./DESIGN_LANGUAGE.md) · [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md) |

**V1 engines (UI selection):** PostgreSQL · MongoDB · SQLite (SQL). Other engines are future connectors.

---

## 1. Executive summary

**Universal Database Studio** is a multi-engine database exploration and modeling tool. Users open the app, choose the database engine they want to work with, connect, and get a consistent workspace: object explorer, data preview, query tools, and a **Model view** that shows structure and connections across the database.

Unlike tools that force a single relational ER model onto every engine, **Model view is universal while its semantics differ by database family**. Relational engines get classic ER (tables + foreign keys). Document and wide-column engines get honest structure maps (collections, inferred references, partition/key models).

**Core bet:** One product shell + pluggable connectors + capability-driven UI beats either (a) SQL-only tools that weakly “support” Mongo/Cassandra or (b) many disconnected single-engine apps.

**Backend bet:** A **Rust** core (fast startup, low memory, single native binary, strong safety) powers connectors, policy, and Model graph building—ideal as a desktop sidecar and optional remote API later.

---

## 2. Problem statement

### 2.1 What we are solving

Teams and individuals work across **many data stores**:

- Relational: PostgreSQL, SQLite, MySQL, cloud SQL (RDS/Aurora, etc.)
- Document: MongoDB, DocumentDB-like services
- Wide-column: Cassandra, Amazon Keyspaces
- Later: DynamoDB, Redis, warehouses, more AWS-managed stores

Today that usually means:

| Pain                              | Description                                                                                                     |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| **Tool sprawl**                   | DBeaver / DataGrip for SQL, Compass for Mongo, cqlsh/DevCenter for Cassandra, AWS consoles for managed services |
| **Inconsistent UX**               | Every tool has different navigation, auth, export, and mental models                                            |
| **Weak structural understanding** | People understand _rows_ but struggle to see _how the database is connected_ (FKs, refs, keys)                  |
| **Unsafe ad-hoc access**          | Raw clients with full write power, no shared policy, little audit                                               |
| **Fake universality**             | Some “universal” tools show SQL-centric UI for non-SQL stores and frustrate users                               |

### 2.2 Job to be done

> When I need to understand or inspect a database, I want to connect quickly, see its structure and relationships, browse data, and run appropriate queries—without switching tools or learning a new UI per engine.

### 2.3 What success looks like

- One app for the engines we claim to support
- After connect, user can answer within minutes: _What objects exist? How are they related? What’s in this table/collection?_
- Model view is always available and **trustworthy** (inferred links labeled; no fake FKs)
- V1 feels excellent on Postgres (+ solid SQLite), not mediocre on fifteen engines

---

## 3. Product vision & principles

### 3.1 Vision

A **studio for data systems**: select engine → connect → explore structure, data, and model—with depth that matches how that database actually works.

### 3.2 Product principles

1. **Engine-honest UX** — Never pretend Mongo is Postgres. Shared shell, different intelligence.
2. **Model view for all** — Every connected database gets a structural/relationship map; ER is the relational variant.
3. **Connector-first architecture** — New engines are plugins, not rewrites.
4. **Safety by default** — Read-oriented V1; writes are explicit and permissioned later.
5. **Depth over logo wallpaper** — Prefer complete Postgres before shallow “supports 20 databases.”
6. **FE presents, BE decides** — UI renders graphs and grids; **Rust** core owns metadata, drivers, policy, secrets.
7. **Lean desktop runtime** — Prefer a native Rust backend binary (and Tauri when possible) over shipping a JVM.

### 3.3 Non-goals (global)

- Replacing full enterprise suites (DataGrip + Liquibase + observability) in V1
- Visual query builders for every language on day one
- Guaranteeing physical ER for engines without relationships
- Competing on IDE-grade refactoring/migrations in early versions

---

## 4. Target users & use cases

### 4.1 Primary users

| Persona                            | Needs                                                     |
| ---------------------------------- | --------------------------------------------------------- |
| **Backend / full-stack developer** | Inspect local & shared DBs, understand schema, debug data |
| **Data engineer**                  | Cross-engine exploration, structure maps, quick queries   |
| **QA / support engineer**          | Read-only inspection, reproduce issues, export samples    |
| **Tech lead / architect**          | Model view for onboarding and design reviews              |

### 4.2 Secondary users

- Students / learners exploring multiple DB types
- Solo builders with SQLite + Postgres + a Mongo side service

### 4.3 Primary use cases

1. Connect to Postgres and understand table relationships via Model view (ER)
2. Browse table data with pagination, open related tables via FK
3. Run SELECT (or engine-equivalent read queries) and export results
4. Connect to SQLite file and inspect schema + data offline
5. Connect to Mongo and see collections + inferred document shape + inferred refs in Model view
6. (Later) Cassandra key model; AWS-managed variants of the above

---

## 5. Product concept

### 5.1 High-level user flow

```
Open application
    → Home: select database engine
    → Connection form (engine-specific)
    → Test connection → Connect
    → Workspace
         ├── Explorer (namespaces → objects → fields)
         ├── Data (preview / grid / documents)
         ├── Query (SQL | MQL | CQL | …)
         └── Model view (structure + connections; semantics per engine)
```

### 5.2 Model view (core differentiator)

**Definition:** A complete structural view of the connected database that shows objects and the connections between them, adapted to the engine.

| Engine family                           | Model view meaning          | Connection semantics                                                                      |
| --------------------------------------- | --------------------------- | ----------------------------------------------------------------------------------------- |
| Relational (Postgres, MySQL, SQLite, …) | Classic **ER diagram**      | Declared foreign keys, PK/UK, cardinality where known                                     |
| Document (MongoDB, …)                   | **Collection / schema map** | Inferred references from field naming & samples; optional manual links; confidence labels |
| Wide-column (Cassandra, …)              | **Table & key model**       | Partition/clustering keys; optional weak links by shared key names                        |
| Key-value / other (later)               | **Access / pattern map**    | GSIs, key patterns—not fake ER                                                            |

**Shared interactions (all engines):**

- Zoom, pan, search nodes
- Click node → open object in Explorer / Data
- Click edge → show relationship detail (FK definition, inferred path, keys)
- Refresh model from live metadata
- Filter (e.g. hide tables without edges, schema filter)

### 5.3 Capability-driven UI

After connect, backend returns **capabilities**. UI shows/hides or renames features:

```json
{
  "engine": "POSTGRES",
  "queryLanguage": "SQL",
  "supportsSchema": true,
  "supportsForeignKeys": true,
  "supportsErDiagram": true,
  "supportsModelView": true,
  "modelViewKind": "ER",
  "supportsPagination": true,
  "supportsWrite": false
}
```

```json
{
  "engine": "MONGODB",
  "queryLanguage": "MQL",
  "supportsCollections": true,
  "supportsDocumentView": true,
  "supportsModelView": true,
  "modelViewKind": "COLLECTION_MAP",
  "supportsInferredRefs": true,
  "supportsWrite": false
}
```

---

## 6. Scope by phase

### 6.1 V1 — “Postgres-first studio with multi-engine foundation”

**Goal:** Ship a polished connect → explore → data → query → model loop for relational core, plus architecture that allows more engines without redesign.

#### V1 in scope

**Platform & shell**

- Desktop application entry point
- Home screen: engine picker
- Connection profiles (save/edit/delete, local secure storage of secrets)
- Test connection + clear error messages
- Single workspace layout (Explorer | Data | Query | Model view)

**Engines**

| Engine                                 | Depth in V1                                                             |
| -------------------------------------- | ----------------------------------------------------------------------- |
| **PostgreSQL**                         | Full target depth                                                       |
| **SQLite**                             | Full relational subset (file-based)                                     |
| **MySQL / MariaDB** (optional stretch) | Same relational module if schedule allows; otherwise first item in Next |

**Features (all V1 engines)**

- Object explorer (schemas/catalogs as applicable, tables/views, columns, indexes basic)
- Data preview with pagination, sort where safe, null rendering, copy cell
- Query editor (SQL) with run, result grid, elapsed time, error display
- Safety: statement timeout, max rows, **read-only mode default** (block non-SELECT for SQL)
- Export results (CSV, JSON)
- **Model view (ER)** from live FK/PK metadata
- Click table in Model view → open Data tab
- Refresh metadata / refresh model
- Dark/light theme (at least one polished default theme)

**MongoDB in V1?**  
**Recommendation: not full Mongo in V1**—only connector SPI stubs + capability types so Mongo is Next without re-architecture.  
_Alternative (if team capacity is high):_ thin Mongo connect + collection list + document preview **without** polished Model view.

**Backend foundation (must ship in V1 even if only 2 engines)** — **Rust**

- `DatabaseConnector` trait (plugin SPI) + per-engine crates/modules
- Normalized types (serde JSON): namespaces, containers, fields, relationships, data pages, model graph
- Model graph builders per engine
- Connection, metadata, and query services (async runtime)
- Audit log of queries (local file or embedded store)—at least query text, engine, timestamp, row count, success/fail
- HTTP API surface (Axum or equivalent) consumable by Svelte desktop UI

**Explicit V1 non-goals**

- Write/edit rows in UI
- Migrations / schema design apply
- Team cloud sync of connection profiles
- Cassandra, DynamoDB, Redis
- AWS IAM auth (basic SSL + user/password is enough)
- Real-time collaboration
- Mobile

#### V1 success metrics

| Metric                                                   | Target                                                                    |
| -------------------------------------------------------- | ------------------------------------------------------------------------- |
| Time to first useful schema view after install           | < 5 minutes on local Postgres                                             |
| Model view correctness (Postgres sample schema with FKs) | All declared FKs visible; no invented edges                               |
| Crash-free connect/browse session                        | No crash on happy path + common errors (bad password, refused connection) |
| Query safety                                             | Non-SELECT rejected in read-only mode                                     |
| Extensibility proof                                      | Second engine (SQLite) uses same SPI as Postgres                          |

---

### 6.2 Next (V1.x / V2) — “Second family + cloud reality”

**Goal:** Prove multi-family UX and production connection patterns.

| Area                        | Scope                                                                                                                            |
| --------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| **MongoDB**                 | Connect, databases/collections, document preview, find queries, inferred schema, **Model view = collection map + inferred refs** |
| **Relational expansion**    | MySQL/MariaDB complete; SQL Server candidate                                                                                     |
| **Model view UX**           | Layout algorithms, collapse schemas, export diagram PNG/SVG, filter isolated nodes                                               |
| **Query UX**                | History, saved queries (local), explain for Postgres                                                                             |
| **AWS (thin)**              | Connection presets for RDS Postgres/MySQL (host/SSL); Secrets Manager optional                                                   |
| **Auth**                    | SSH tunnel; SSL cert modes; improve secret storage                                                                               |
| **Writes (optional gated)** | Explicit “allow writes” per connection; single-row edit for relational only                                                      |
| **Quality**                 | Integration tests per connector; larger result streaming/cancel                                                                  |

**Next non-goals**

- Full Compass/DataGrip parity
- Multi-user SaaS control plane (unless product pivots)

---

### 6.3 Future — “Universal studio”

| Area                      | Scope                                                                       |
| ------------------------- | --------------------------------------------------------------------------- |
| **Cassandra / Keyspaces** | CQL editor, key model Model view                                            |
| **DynamoDB**              | Table/GSI map, item browser, query/scan with strict guards                  |
| **Redis**                 | Key browser, pattern groups, limited model                                  |
| **Warehouses**            | Redshift/BigQuery/Snowflake-style read exploration                          |
| **Graph DBs**             | Neo4j as separate model language                                            |
| **Team / cloud product**  | Hosted API, SSO, shared read-only roles, org audit                          |
| **AI assist (optional)**  | Natural language → query draft; schema Q&A (never auto-run destructive SQL) |
| **Web client**            | Same API as desktop; browser UI in Svelte                                   |
| **Import**                | CSV import into tables (permissioned)                                       |
| **Compare**               | Schema diff across environments                                             |
| **ER design mode**        | Edit model and generate DDL (relational only)—advanced                      |

---

## 7. Functional requirements

### 7.1 Engine selection & connections

| ID  | Requirement                                              | Priority |
| --- | -------------------------------------------------------- | -------- |
| C-1 | User can select engine from Home before connecting       | P0       |
| C-2 | Connection form fields depend on engine                  | P0       |
| C-3 | User can test connection without fully opening workspace | P0       |
| C-4 | User can save named connection profiles                  | P0       |
| C-5 | Secrets are not stored in plain text at rest on disk     | P0       |
| C-6 | User can edit/delete/duplicate profiles                  | P1       |
| C-7 | Connection errors are human-readable                     | P0       |

### 7.2 Explorer & metadata

| ID  | Requirement                                             | Priority |
| --- | ------------------------------------------------------- | -------- |
| M-1 | After connect, explorer shows namespaces and containers | P0       |
| M-2 | Expanding a table/collection shows fields/types         | P0       |
| M-3 | User can refresh metadata                               | P0       |
| M-4 | Views listed separately or badged for SQL engines       | P1       |
| M-5 | Indexes visible in detail panel                         | P1       |

### 7.3 Data view

| ID  | Requirement                                                | Priority            |
| --- | ---------------------------------------------------------- | ------------------- |
| D-1 | Paginated data preview for tables/collections              | P0                  |
| D-2 | Configurable page size with server-side max cap            | P0                  |
| D-3 | NULL and type-aware rendering (json, bool, dates)          | P1                  |
| D-4 | Copy cell / copy row                                       | P1                  |
| D-5 | Export current result CSV/JSON                             | P0                  |
| D-6 | FK cell navigation (relational): jump to related row/table | P2 (Next if needed) |

### 7.4 Query

| ID  | Requirement                                                      | Priority |
| --- | ---------------------------------------------------------------- | -------- |
| Q-1 | Engine-appropriate query editor                                  | P0       |
| Q-2 | Run query; show rows, columns, timing, errors                    | P0       |
| Q-3 | Read-only policy for V1 SQL (SELECT only; block multi-statement) | P0       |
| Q-4 | Max rows and timeout enforced server-side                        | P0       |
| Q-5 | Query history (local)                                            | P1       |
| Q-6 | Cancel in-flight query                                           | P2       |

### 7.5 Model view

| ID  | Requirement                                                                 | Priority                     |
| --- | --------------------------------------------------------------------------- | ---------------------------- |
| V-1 | Model view available for every successfully connected engine in scope       | P0                           |
| V-2 | Relational engines render ER from declared PK/FK metadata only              | P0                           |
| V-3 | Edges are not invented for relational ER                                    | P0                           |
| V-4 | Node click opens object data/details                                        | P0                           |
| V-5 | Edge click shows relationship metadata                                      | P0                           |
| V-6 | Layout is readable for medium schemas (e.g. ≤ 80 tables) with search/filter | P0                           |
| V-7 | Non-relational engines use documented alternate model semantics             | P0 (when those engines ship) |
| V-8 | Inferred edges labeled with confidence/source                               | P0 (Mongo+)                  |
| V-9 | Export diagram image                                                        | P2                           |

### 7.6 Cross-cutting

| ID  | Requirement                                                            | Priority |
| --- | ---------------------------------------------------------------------- | -------- |
| X-1 | Capability flags drive visible tabs/actions                            | P0       |
| X-2 | All query/metadata operations go through backend (no raw driver in UI) | P0       |
| X-3 | Structured logging / local query audit                                 | P1       |
| X-4 | App usable offline for SQLite and local Postgres                       | P0       |

---

## 8. Non-functional requirements

| Area                 | Requirement                                                                                                                          |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| **Performance**      | Metadata load for 200-table Postgres schema < 5s on typical laptop; data page < 2s for simple table                                  |
| **Safety**           | Defaults read-only; destructive operations require future explicit enablement                                                        |
| **Security**         | Secrets encrypted via OS keychain / safe storage; no secrets in renderer logs                                                        |
| **Reliability**      | Connector failures isolated; one bad engine doesn’t crash shell                                                                      |
| **Extensibility**    | New engine = new connector module + model builder + form schema, without changing core DTOs drastically                              |
| **UX**               | Consistent layout across engines; engine differences explained in empty states                                                       |
| **Packaging**        | Installable desktop artifact for at least one major OS in V1; **native Rust binary** (no JVM). Define target OS in engineering plan. |
| **Startup / memory** | Local backend ready quickly; prefer low RSS suitable for always-on desktop sidecar                                                   |

---

## 9. Technology choices

### 9.1 Recommended stack

| Layer                     | Choice                                                                                                  | Rationale                                                                                                           |
| ------------------------- | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| **UI framework**          | **Svelte 5 + Vite**                                                                                     | Lightweight, excellent for dense tools (grids, panels, diagrams)                                                    |
| **Diagram / Model view**  | **Svelte Flow (xyflow)** + layout (dagre or ELK)                                                        | Purpose-built node/edge UI; good for ER and maps                                                                    |
| **Desktop shell**         | **Tauri 2** (preferred) or **Electron**                                                                 | Tauri pairs naturally with Rust core; smaller footprint. Electron still viable (Svelte talks HTTP to Rust sidecar). |
| **Backend**               | **Rust** (edition 2021+)                                                                                | Native binary, fast startup, low memory, memory safety; strong fit for desktop sidecar and multi-connector core     |
| **HTTP framework**        | **Axum** (+ Tower, Hyper)                                                                               | Idiomatic async API; middleware for timeouts, tracing, CORS (localhost)                                             |
| **Async runtime**         | **Tokio**                                                                                               | De-facto standard for DB drivers and networking                                                                     |
| **API style**             | **REST + JSON** (OpenAPI via `utoipa` or similar); optional WebSocket later for cancel/stream           | Simple for Svelte client; optional TS typegen from OpenAPI                                                          |
| **Serialization**         | **serde** / **serde_json**                                                                              | Shared DTOs for metadata, query results, model graph                                                                |
| **SQL (Postgres)**        | **tokio-postgres** and/or **sqlx** (Postgres)                                                           | Async queries + metadata introspection                                                                              |
| **Connection pool (SQL)** | **deadpool** / **bb8** / sqlx pool                                                                      | Pooling without JDBC                                                                                                |
| **SQLite**                | **sqlx** (SQLite) or **rusqlite**                                                                       | File-based connector; great offline desktop story                                                                   |
| **Mongo (Next)**          | **mongodb** (official Rust driver)                                                                      | Document family                                                                                                     |
| **Cassandra (Future)**    | **scylla-rust-driver** (or equivalent)                                                                  | Wide-column family                                                                                                  |
| **Validation / policy**   | Explicit query guards + schema validation (`validator` / custom)                                        | Read-only SQL, max rows, timeouts                                                                                   |
| **Secrets**               | OS keychain via Tauri plugins or `keyring` crate; never plain config files                              | Desktop-safe secret storage                                                                                         |
| **Observability**         | `tracing` + structured logs                                                                             | Debug connectors and query audit                                                                                    |
| **Build**                 | **Cargo** workspaces (BE), pnpm/npm (FE)                                                                | Multi-crate connectors (`connectors-postgres`, `connectors-sqlite`, …)                                              |
| **Testing**               | `cargo test`, integration tests with Testcontainers-rs / Docker Postgres & Mongo; FE unit/e2e as needed | Connector truth                                                                                                     |

### 9.2 Architecture diagram

```
┌─────────────────────────────────────────────────────────┐
│  Desktop shell (Tauri preferred, or Electron)           │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Svelte UI                                        │  │
│  │  Home · Connect · Explorer · Data · Query · Model │  │
│  └─────────────────────────┬─────────────────────────┘  │
│                            │ HTTP (127.0.0.1) or IPC    │
└────────────────────────────┼────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────┐
│  Rust core (Axum + Tokio)                               │
│  ├── Connection profiles & secret refs                  │
│  ├── Query policy (timeout, max rows, read-only)        │
│  ├── Metadata service                                   │
│  ├── Model graph service                                │
│  └── Connector trait (SPI)                              │
│        ├── postgres + PostgresModelBuilder              │
│        ├── sqlite + SqliteModelBuilder                  │
│        ├── mongodb + MongoModelBuilder (Next)           │
│        └── …                                            │
└────────────────────────────┬────────────────────────────┘
                             ▼
                    Target databases
```

**Tauri-native option (V1 or soon after):** embed or invoke Rust commands from the Tauri side for some paths, while keeping the same connector/domain crates so a standalone HTTP server remains possible for remote/web later.

### 9.3 Deployment topologies

| Mode                    | Description                                                                              | V1?                   |
| ----------------------- | ---------------------------------------------------------------------------------------- | --------------------- |
| **Local native binary** | Desktop starts/manages Rust studio server on `127.0.0.1`, or Tauri hosts core in-process | Primary               |
| **Remote API**          | Same Rust binary/service deployed for team; desktop/web as clients                       | Future / team edition |
| **Web-only**            | Browser + hosted Rust API                                                                | Future                |

### 9.4 Alternative tech options (considered)

| Alternative            | When to choose instead                                                                       |
| ---------------------- | -------------------------------------------------------------------------------------------- |
| **Spring Boot / Java** | Team is Java-heavy and needs maximum JDBC driver breadth fastest; accepts JVM packaging cost |
| Electron over Tauri    | Team wants pure web main process; still call Rust over localhost HTTP                        |
| Node BE                | TS-only team, fewer engines; weaker native packaging and multi-driver long game              |
| SvelteKit full-stack   | Web-first product; desktop secondary                                                         |
| Single SQL crate only  | Fails for Mongo/Cassandra—connector trait still required                                     |

### 9.5 Why Rust (decision record)

| Factor            | Decision impact                                                                                     |
| ----------------- | --------------------------------------------------------------------------------------------------- |
| Desktop packaging | Ship one (or few) native binaries; **no JRE**                                                       |
| Resource use      | Better default for always-on local sidecar                                                          |
| Safety            | Aligns with read-only policy and careful connection handling                                        |
| Ecosystem         | Excellent for Postgres/SQLite; Mongo/Cassandra available with more integration work than Java       |
| Velocity risk     | Connector work may be slower than Spring/JDBC—**mitigated by Postgres-first V1 and SPI discipline** |

### 9.6 Model graph DTO (contract sketch)

```text
ModelGraph {
  engine, modelViewKind, generatedAt
  nodes[]: { id, name, namespace, kind, fields[], badges[] }
  edges[]: { id, fromNodeId, toNodeId, kind, label, confidence, detail }
}

edge.kind: FK | INFERRED | SHARED_KEY | MANUAL | ACCESS_PATH
confidence: CONFIRMED | HIGH | MEDIUM | LOW  (CONFIRMED for declared FKs)
```

Serialized as JSON via serde; Svelte consumes the same contract regardless of engine.

---

## 10. Pros and cons

### 10.1 Product approach (universal shell + per-engine model)

| Pros                                   | Cons                                                                       |
| -------------------------------------- | -------------------------------------------------------------------------- |
| One app / one learning curve           | Harder UX copy—must explain “Model” ≠ always ER                            |
| Honest multi-DB story builds trust     | Users may expect DBeaver depth immediately                                 |
| Model view is a clear differentiator   | Diagram UX is expensive to polish                                          |
| Capability flags scale to many engines | Incomplete connectors look like product bugs if marketed as “full support” |
| Safe default read path                 | Power users will demand writes/admin features early                        |

### 10.2 Svelte + Rust + desktop shell

| Pros                                                 | Cons                                                                      |
| ---------------------------------------------------- | ------------------------------------------------------------------------- |
| Svelte: fast UI iteration for complex tools          | Two languages in the stack (TS/Svelte + Rust)—hiring/skills split         |
| Rust: single native binary, fast startup, low memory | Fewer “batteries included” than Spring for enterprise auth/admin patterns |
| Clear FE/BE boundary via HTTP or Tauri commands      | Async + connector edge cases have steeper learning curve                  |
| Same API can power future web client                 | Some engines/drivers less mature than Java JDBC ecosystem                 |
| Excellent fit with **Tauri** (shared Rust world)     | Multi-engine velocity may lag a Java team that already knows JDBC         |
| No JVM install/bundling pain                         | Build times and cross-compile CI need investment                          |

### 10.3 Postgres-first V1

| Pros                                               | Cons                                               |
| -------------------------------------------------- | -------------------------------------------------- |
| Best ER metadata story                             | Mongo/Cassandra users wait                         |
| Validates core loop before multi-family complexity | Marketing must not claim “all databases” at launch |
| SQLite proves second connector cheaply             | Risk of over-fitting UI to SQL                     |
| Strong Rust crate support for Postgres + SQLite    | —                                                  |

### 10.4 Model view for every engine (adapted semantics)

| Pros                                  | Cons                                                     |
| ------------------------------------- | -------------------------------------------------------- |
| Universal navigation concept          | Inferred Mongo edges can be wrong—must label clearly     |
| Better onboarding than raw tree alone | Layout algorithms struggle on huge schemas (500+ tables) |
| Click-to-data is high leverage        | Cassandra “connections” may feel thin vs ER              |

### 10.5 Risks and mitigations

| Risk                                   | Mitigation                                                                                 |
| -------------------------------------- | ------------------------------------------------------------------------------------------ |
| Scope explosion (“support everything”) | Phased engines; connector trait first; depth metrics per engine                            |
| Fake relationships erode trust         | CONFIRMED vs INFERRED; no silent invention on SQL                                          |
| Rust connector velocity                | Postgres + SQLite V1 only; checklist before “Supported”; reuse shared SQL metadata helpers |
| Cross-compile / packaging              | CI matrix early; prefer Tauri bundling story; document glibc/musl targets                  |
| Huge schemas unusable in diagram       | Search, filter, subgraph by schema, “focus neighbors”                                      |
| Security incident via query feature    | Read-only default, timeouts, max rows, audit, no multi-statement                           |
| Connector quality variance             | Per-engine acceptance checklist before “Supported” badge                                   |
| Team Rust fluency                      | Spike M0 on Axum + one Postgres connector before locking schedule                          |

---

## 11. Competitive context (brief)

| Tool           | Strength              | Gap we exploit                                   |
| -------------- | --------------------- | ------------------------------------------------ |
| DBeaver        | Broad engine coverage | Heavy UX; model/ER quality varies; not FE-modern |
| DataGrip       | Excellent SQL IDE     | Paid; less friendly multi-family model story     |
| Mongo Compass  | Great Mongo           | Single family                                    |
| cloud consoles | Managed ops           | Fragmented; not unified local studio             |
| TablePlus      | Fast polished UI      | Narrower engine/modeling depth                   |

**Positioning:** Modern, capability-honest multi-engine studio with **Model view as the structural center**, not only a SQL editor with a tree.

---

## 12. Milestone plan (indicative)

| Milestone                    | Outcome                                                                                                |
| ---------------------------- | ------------------------------------------------------------------------------------------------------ |
| **M0 — Foundations**         | Cargo workspace, connector trait + DTOs (serde), Axum app, Svelte shell, engine picker, mock connector |
| **M1 — Postgres connect**    | Profiles, metadata explorer, data page, SQL read-only (`tokio-postgres` / sqlx)                        |
| **M2 — Postgres Model view** | ER graph from FK metadata, navigation, layout                                                          |
| **M3 — SQLite**              | File connect + same relational UI                                                                      |
| **M4 — V1 harden**           | Export, errors, audit, native packaging (Tauri/installer), docs                                        |
| **M5 — Mongo Next**          | `mongodb` driver + collection model view                                                               |
| **M6+**                      | MySQL, AWS presets, Cassandra, writes, remote/web                                                      |

---

## 13. Open decisions (to resolve at kickoff)

1. **Desktop shell:** Tauri (recommended with Rust) vs Electron + Rust HTTP sidecar
2. **V1 OS targets:** Windows / macOS / Linux priority order
3. **MySQL in V1 or Next?**
4. **Mongo thin slice in V1 or strictly Next?**
5. **Process model:** Tauri in-process Rust commands vs always-on local HTTP sidecar (or both)
6. **SQL crate choice:** sqlx vs tokio-postgres (+ rusqlite) for V1 relational connectors
7. **Product name** (Universal Database Studio is working title only)
8. **Licensing model:** open source vs freemium vs internal tool

---

## 14. Acceptance checklist for calling an engine “Supported”

An engine may be marketed as supported only if:

- [ ] Connect + test connection documented
- [ ] Explorer lists major object types correctly
- [ ] Data preview works with pagination caps
- [ ] Query path works for that engine’s read language with safety policy
- [ ] Model view returns a graph with documented semantics
- [ ] Failure modes tested (auth fail, timeout, network)
- [ ] Automated tests (at least smoke + metadata golden path)

---

## 15. Summary decision

| Topic                       | Decision                                                                                           |
| --------------------------- | -------------------------------------------------------------------------------------------------- |
| **Idea**                    | Multi-engine DB studio: select → connect → explore / query / **Model view**                        |
| **Solving**                 | Tool sprawl, weak structural understanding, dishonest “universal” UIs                              |
| **Backend**                 | **Rust** (Axum + Tokio + connector trait); native binary / Tauri-friendly                          |
| **Frontend**                | **Svelte 5** + Svelte Flow for Model view                                                          |
| **V1**                      | Desktop + Svelte + Rust; Postgres + SQLite; relational ER Model view; read-only SQL; connector SPI |
| **Next**                    | Mongo model map, more SQL engines, AWS connection UX, query history, diagram polish                |
| **Future**                  | Cassandra, DynamoDB, team/cloud, web client, AI assist, design/DDL modes                           |
| **ER policy**               | Differs by DB; universal **Model view**, relational **ER** as one kind                             |
| **Rejected for primary BE** | Spring Boot / JVM (kept only as historical alternative in §9.4)                                    |

---

## 16. Document history

| Version | Date       | Notes                                                                                                                 |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------- |
| 1.0     | 2026-07-25 | Initial PRD from product discovery (multi-DB studio, Model view, Svelte + Java/Spring, phased engines)                |
| 1.1     | 2026-07-25 | Backend decision: **Rust** instead of Spring Boot; stack, architecture, pros/cons, milestones, open decisions updated |
| 1.2     | 2026-07-25 | Branded **MystiqueDB** in-repo; V1 UI engines Postgres/Mongo/SQLite; linked [`KANBAN.md`](./KANBAN.md)                 |

---

_End of PRD_
