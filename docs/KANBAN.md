# MystiqueDB — Project Kanban

> **Status:** Living board (update columns when tickets move)  
> **Visual board (open in browser):** [`kanban.html`](./kanban.html)  
> **Sources:** [`PRD.md`](./PRD.md) · [`ARCHITECTURE_AND_FEATURES.md`](./ARCHITECTURE_AND_FEATURES.md) · [`FEATURES.md`](./FEATURES.md) · [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md) · [`BACKEND_STRUCTURE.md`](./BACKEND_STRUCTURE.md)  
> **Last updated:** 2026-07-26  
> **V1 engines:** PostgreSQL · MongoDB · MySQL · **Policy:** per-engine `EngineQueryPolicy` SPI (read default · write opt-in · DELETE off)

## View the board

```bash
# from repo root — open the HTML board in your browser
xdg-open docs/kanban.html   # Linux
# open docs/kanban.html     # macOS
# start docs/kanban.html    # Windows
```

Or double-click: **`mystique-db/docs/kanban.html`**

This markdown file is the text source; **`kanban.html`** is the visual kanban (TODO / Progress / QA / Deployed).

## How to use

| Column | Meaning |
| ------ | ------- |
| **TODO** | Not started |
| **Progress** | Actively in development |
| **QA** | Implementation done; needs verification |
| **Deployed** | Shipped / accepted in main |

### Tags

| Tag | Meaning |
| --- | ------- |
| `FE` | Frontend (Svelte / UI) |
| `BE` | Backend (Rust / Tauri / connectors) |
| `DOCS` | Documentation only |
| `P0` / `P1` / `P2` / `P3` | Priority (from PRD / FEATURES) |

### Ticket fields

Each ticket: **ID**, **Title**, **Priority**, **Tags**, **Description**, **Related** (doc refs / depends on).

---

## Deployed

### T-BASE-001 — Tauri + SvelteKit scaffold
- **Priority:** P0 · **Tags:** `FE` `BE`
- **Description:** Desktop shell with Vite/SvelteKit SPA, Tauri 2 window, icons, dev/build scripts.
- **Related:** README · Initial commit

### T-BASE-002 — Design system primitives
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Semantic tokens, light/dark theme, UI primitives (Button, Field, Card, …) per design language.
- **Related:** DESIGN_LANGUAGE.md · UI_UX_GUIDELINES.md · `src/lib/components/ui/*`

### T-BASE-003 — Local app SQLite + profile CRUD
- **Priority:** P0 · **Tags:** `BE`
- **Description:** `mystique-core` domain/storage/application; migrations; IPC for profiles, query library, history, settings.
- **Related:** SQLITE_SCHEMA.md · BACKEND_STRUCTURE.md

### T-BASE-004 — Home connection rail + detail/edit UI
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Left rail list, connection detail, new/edit forms wired to profile APIs (and localStorage FE fallback).
- **Related:** FEATURES F-CONN-* · home components

### T-BASE-005 — Product docs set
- **Priority:** P0 · **Tags:** `DOCS`
- **Description:** PRD, architecture, features, UI/UX, design language, component structure, schema docs in `docs/`.
- **Related:** docs/*

### T-DOCS-META — LICENSE, CHANGELOG, CONTRIBUTING, Wiki
- **Priority:** P1 · **Tags:** `DOCS`
- **Description:** MIT LICENSE; Keep a Changelog; general GitLab CONTRIBUTING; `docs/wiki/` pages for GitLab Wiki.
- **Related:** LICENSE · CHANGELOG.md · CONTRIBUTING.md · docs/wiki/


### T-BE-001 — Connector API crate (`mystique-connector-api`)
- **Priority:** P0 · **Tags:** `BE` · **Status:** Deployed
- **Description:** Traits, DTOs, pool types. No engine drivers in API crate.

### T-BE-002 — Connector registry
- **Priority:** P0 · **Tags:** `BE` · **Status:** Deployed
- **Description:** `ConnectorRegistry::bootstrap()` registers postgres/sqlite/mongodb.

### T-BE-004 — IPC surface for engines & sessions
- **Priority:** P0 · **Tags:** `BE` `FE` · **Status:** Deployed
- **Description:** list/test/open/close/ping/pool-status/list sessions.

### T-BE-POOL — Engine connection pools
- **Priority:** P0 · **Tags:** `BE` · **Status:** Deployed
- **Description:** deadpool (PG), r2d2 (SQLite), driver pool (Mongo).

### T-DEV-MONGO — Local Mongo docker seed
- **Priority:** P1 · **Tags:** `BE` `DOCS` · **Status:** Deployed
- **Description:** mongo:7 :27017 user abc/abc, DB mystique_dev sample collections. See DEV_MONGO.md.

### T-FE-001 — Engine picker (Postgres / Mongo / SQLite)
- **Priority:** P0 · **Tags:** `FE`
- **Description:** First step of new connection: choose database engine with design-language cards. Only PostgreSQL, MongoDB, and SQLite (SQL). No live BE plugins yet.
- **Related:** PRD §5 user flow · FEATURES F-CONN · NewConnectionPanel (engine dropdown)

---

## Progress

_(empty — move tickets here when work starts)_

---

## QA

_(empty — move tickets here when ready for review)_

---

## TODO

### Foundation / connector base (M0)

#### T-BE-001 — Connector API crate (`mystique-connector-api`)
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Traits `DatabaseConnector` + `ConnectorSession`, capabilities, errors, normalized DTOs (Namespace, Container, Field, Relationship, DataPage, ModelGraph). No engine drivers yet.
- **Related:** PRD §9 · ARCHITECTURE §4 · depends on: —

#### T-BE-002 — Connector registry + mock connector
- **Priority:** P0 · **Tags:** `BE`
- **Description:** `ConnectorRegistry` loads plugins; mock connector proves SPI for tests and FE capability chrome.
- **Related:** ARCHITECTURE §4.4 · depends on: T-BE-001

#### T-BE-003 — Per-engine query policy SPI + modes + limits
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Introduce `EngineQueryPolicy` in `mystique-connector-api` (shared request/result types only). Core orchestrates: resolve policy by `engine_id` → `validate` → execute. **Modes:** Read (default), Write (opt-in checkbox), **DELETE always denied**. Shared **max rows + timeout** hard caps. Fail closed if unclassified. **Not** a single global SQL allowlist — each engine plugin owns its policy impl. Session open/close already deployed (T-BE-004 / T-BE-POOL).
- **Related:** PRD Q-3 / Q-3a–d / Q-4 / Q-7 · ARCHITECTURE §6.4 · BACKEND_STRUCTURE · depends on: T-BE-001

#### T-BE-004 — IPC surface for engines & sessions
- **Priority:** P0 · **Tags:** `BE` `FE` · **Status:** Deployed (see Deployed column / kanban.html)
- **Description:** Tauri commands: list/test/open/close/ping/pool-status/list sessions.
- **Related:** ARCHITECTURE §10 · depends on: T-BE-002

#### T-BE-005 — OS keychain for secrets
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Store passwords under `secret_ref`; never plaintext in SQLite; ProfileService hooks.
- **Related:** PRD C-5 · profiles.rs comments · depends on: T-BASE-003

#### T-BE-006 — Metadata / data / query / model core services
- **Priority:** P0 · **Tags:** `BE`
- **Description:** MetadataService, DataService, **QueryService** (policy → execute → history), ModelGraphService, ExportService, AuditLog.
- **Related:** ARCHITECTURE §5–6 · depends on: T-BE-001–004, T-BE-003

#### T-FE-002 — Capability-driven workspace chrome
- **Priority:** P0 · **Tags:** `FE`
- **Description:** After connect, show/hide Explorer, Data, Query, Model tabs from `EngineCapabilities`; engine-correct nouns (table vs collection).
- **Related:** PRD §5.3 · UI_UX §8.6 · depends on: T-BE-004

### PostgreSQL (V1)

#### T-BE-010 — `mystique-plugin-postgres` connect + metadata
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Connect/pool; list schemas, tables, columns, indexes, FKs.
- **Related:** PRD V1 · depends on: T-BE-001

#### T-BE-011 — Postgres data page + SQL execute + PostgresQueryPolicy
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Paginated `fetch_data_page`; `execute_query` only after **PostgresQueryPolicy** (read: SELECT/WITH; write: INSERT/UPDATE; DELETE/TRUNCATE deny; multi-statement deny); history append.
- **Related:** PRD D-*, Q-* · depends on: T-BE-010, T-BE-003

#### T-BE-012 — Postgres Model graph (ER from FKs)
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Build ModelGraph from declared PK/FK only; no invented edges; confidence CONFIRMED.
- **Related:** PRD V-1–V-6 · depends on: T-BE-010

#### T-FE-010 — Explorer tree (relational)
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Schemas → tables/views → columns; search; open data tab.
- **Related:** depends on: T-BE-010, T-FE-003

#### T-FE-011 — Data grid + pagination
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Dense grid, null rendering, page size, sticky header.
- **Related:** PRD D-* · depends on: T-BE-011

#### T-FE-012 — Query space (editor + run + results)
- **Priority:** P0 · **Tags:** `FE` · **Status:** Deployed (FE canvas; BE execute later)
- **Description:** Query canvas opens on connect for any engine. Editor + Run + **Allow writes** toggle; FE policy stub blocks DELETE / read-only violations. Live driver execute still T-BE-011 / T-BE-003.
- **Related:** QueryCanvas.svelte · queryPolicyFe.ts · F-SHELL-15

#### T-FE-014 — Export results CSV/JSON
- **Priority:** P0 · **Tags:** `FE` `BE`
- **Description:** Export current grid/result set.
- **Related:** PRD D-5 · depends on: T-FE-011

### MySQL (V1)

#### T-BE-050 — `mystique-plugin-mysql` + MysqlQueryPolicy
- **Priority:** P0 · **Tags:** `BE`
- **Description:** V1 target. Connect/pool, metadata, data page, execute. **Own MySQL dialect policy** (not a fork of Postgres policy). Same mode rules: read default, write opt-in, DELETE denied.
- **Related:** PRD V1 engines · ARCHITECTURE §6.4 · depends on: T-BE-001, T-BE-003

### MongoDB (V1)

#### T-BE-030 — `mystique-plugin-mongodb` + MongoQueryPolicy
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Connect, list DBs/collections, document preview, find/aggregate; **MongoQueryPolicy** (read default; write opt-in; delete/drop denied).
- **Related:** PRD V1 · ARCHITECTURE §6.4 · depends on: T-BE-001, T-BE-003

#### T-BE-031 — Mongo Model view builder (inferred refs)
- **Priority:** P1 · **Tags:** `BE`
- **Description:** Collection map + inferred edges labeled with confidence; never claim FK.
- **Related:** depends on: T-BE-030

#### T-FE-030 — Document list + JSON pretty viewer
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Pretty JSON / document results for Mongo query space and data preview.
- **Related:** FEATURES F-JSON · depends on: T-BE-030

#### T-FE-031 — Mongo Model view UI
- **Priority:** P1 · **Tags:** `FE`
- **Description:** Same canvas shell as ER; collection node chrome.
- **Related:** depends on: T-BE-031, T-FE-013

### SQLite (later — not V1 target)

#### T-BE-020 — `mystique-plugin-sqlite` + SqliteQueryPolicy
- **Priority:** P2 · **Tags:** `BE`
- **Description:** File DBs; own policy impl when prioritized. Scaffold pool may already exist.
- **Related:** PRD Next · depends on: T-BE-001, T-BE-003

#### T-FE-020 — SQLite form polish + file picker
- **Priority:** P2 · **Tags:** `FE`
- **Description:** Native file dialog for path.
- **Related:** depends on: T-BE-020

### Product features (from FEATURES.md)

#### T-FE-040 — Browser-like workspace tabs
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Unlimited tabs per connection; types: table, query, model; dirty indicator; overflow menu.
- **Related:** F-TAB-1–4 · depends on: T-FE-003

#### T-FE-041 — Row detail right panel
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Vertical field list for selected row; prev/next; JSON expand; resize/collapse.
- **Related:** F-ROW-1–6 · depends on: T-FE-011

#### T-FE-042 — Edit / insert modals (gated writes)
- **Priority:** P0 · **Tags:** `FE` `BE`
- **Description:** Row edit/insert modal, confirm before write, respect read-only.
- **Related:** F-EDIT-* · depends on: T-FE-011 · policy writes later

#### T-FE-043 — Saved queries UI (groups per connection)
- **Priority:** P0 · **Tags:** `FE`
- **Description:** UI on top of existing query_library BE; open in tab; rename/move/delete.
- **Related:** F-SQ-* · BE already partial (T-BASE-003) · depends on: T-FE-012

#### T-FE-044 — Query history UI
- **Priority:** P1 · **Tags:** `FE`
- **Description:** Browse past runs from history tables; re-run; filter by connection.
- **Related:** FEATURES history · depends on: T-BASE-003, T-FE-012

#### T-FE-045 — Collapse connection rail
- **Priority:** P1 · **Tags:** `FE`
- **Description:** Icon-only sidebar mode.
- **Related:** F-CONN-5 · depends on: T-BASE-004

### Quality & packaging

#### T-BE-040 — Integration tests (Postgres + policy cases)
- **Priority:** P1 · **Tags:** `BE`
- **Description:** Smoke connect, metadata, ER edges; policy: read rejects writes; write rejects DELETE; multi-statement rejected.
- **Related:** PRD §14 · depends on: T-BE-010–012, T-BE-003

#### T-FE-050 — E2E / typecheck CI for UI
- **Priority:** P1 · **Tags:** `FE`
- **Description:** `yarn check` in CI; optional Playwright smoke for engine picker + form.
- **Related:** depends on: T-FE-001

#### T-BE-041 — Desktop packaging (Tauri release)
- **Priority:** P1 · **Tags:** `BE` `FE`
- **Description:** Signed/unsigned installers for target OS; native Rust binary, no JVM.
- **Related:** PRD packaging · depends on: M1 loop

### Later / P2+

#### T-BE-051 — AWS RDS connection presets + SSL/IAM
- **Priority:** P2 · **Tags:** `BE` `FE`
- **Description:** Connection form presets for managed Postgres/MySQL; optional Secrets Manager later.
- **Related:** PRD Next AWS · depends on: T-BE-010, T-BE-050

#### T-BE-052 — Cassandra / Keyspaces plugin
- **Priority:** P2 · **Tags:** `BE`
- **Description:** CQL + key-model Model view.
- **Related:** PRD Future · depends on: T-BE-001

#### T-FE-060 — Diagram export PNG/SVG
- **Priority:** P2 · **Tags:** `FE`
- **Description:** Export Model view.
- **Related:** F-REL-8 · depends on: T-FE-013

#### T-FE-061 — Session restore tabs
- **Priority:** P1 · **Tags:** `FE` `BE`
- **Description:** Restore open tabs after restart.
- **Related:** F-TAB-5 · depends on: T-FE-040

#### T-PROD-001 — Command palette
- **Priority:** P2 · **Tags:** `FE`
- **Description:** ⌘/Ctrl+K jump connections, tables, commands.
- **Related:** UI_UX §8.2 · depends on: T-FE-003

---

## Suggested near-term order

1. **T-BE-003** — Policy SPI + modes + limits (session IPC already deployed)  
2. **T-BE-010 → T-BE-011** — Postgres metadata + execute **with PostgresQueryPolicy**  
3. **T-FE-012** — Thin query space (run + results + allow-writes checkbox)  
4. **T-BE-050** — MySQL plugin + **MysqlQueryPolicy**  
5. **T-BE-030** — Mongo plugin + **MongoQueryPolicy**  
6. Explorer/data grid polish · model depth · keychain (T-BE-005) as parallel  

Canonical board data: **`docs/kanban.html`**. Keep this file aligned when tickets move.

---

## Board snapshot

| Column | Count (approx.) |
| ------ | --------------- |
| Deployed | ~28 (see kanban.html) |
| Progress | 0 |
| QA | 0 |
| TODO | ~30 |

Update **kanban.html** (and this file) when starting/finishing work.

### T-FE-TEST — Test connection FE → BE
- **Priority:** P0 · **Tags:** `FE` `BE` · **Status:** Deployed
- **Description:** Test connection invokes backend plugins; success shows latency + server version; failure shows error. Connect on detail reuses the same path.
