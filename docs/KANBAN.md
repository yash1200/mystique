# MystiqueDB — Project Kanban

> **Status:** Living board (update columns when tickets move)  
> **Visual board (open in browser):** [`kanban.html`](./kanban.html)  
> **Sources:** [`PRD.md`](./PRD.md) · [`ARCHITECTURE_AND_FEATURES.md`](./ARCHITECTURE_AND_FEATURES.md) · [`FEATURES.md`](./FEATURES.md) · [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md) · [`BACKEND_STRUCTURE.md`](./BACKEND_STRUCTURE.md)  
> **Last updated:** 2026-07-25

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
- **Related:** PRD §5 user flow · FEATURES F-CONN · EnginePicker.svelte · NewConnectionPanel locked engine

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

#### T-BE-003 — Session manager + query policy stubs
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Connect/disconnect session lifecycle; read-only SQL policy, timeout, max rows (enforce before plugins).
- **Related:** PRD Q-3/Q-4 · ARCHITECTURE §6 · depends on: T-BE-001

#### T-BE-004 — IPC surface for engines & sessions
- **Priority:** P0 · **Tags:** `BE` `FE`
- **Description:** Tauri commands: `list_engines`, `test_connection`, `connect`, `disconnect`, `get_capabilities`.
- **Related:** ARCHITECTURE §10 · depends on: T-BE-002, T-BE-003

#### T-BE-005 — OS keychain for secrets
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Store passwords under `secret_ref`; never plaintext in SQLite; ProfileService hooks.
- **Related:** PRD C-5 · profiles.rs comments · depends on: T-BASE-003

#### T-BE-006 — Metadata / data / query / model core services
- **Priority:** P0 · **Tags:** `BE`
- **Description:** MetadataService, DataService, QueryService, ModelGraphService, ExportService, AuditLog orchestrating connectors.
- **Related:** ARCHITECTURE §5–6 · depends on: T-BE-001–004

#### T-FE-002 — Capability-driven workspace chrome
- **Priority:** P0 · **Tags:** `FE`
- **Description:** After connect, show/hide Explorer, Data, Query, Model tabs from `EngineCapabilities`; engine-correct nouns (table vs collection).
- **Related:** PRD §5.3 · UI_UX §8.6 · depends on: T-BE-004

#### T-FE-003 — App workspace shell layout
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Connection tabs + workspace tabs + object tree region + main pane + status bar per UI guidelines shell.
- **Related:** UI_UX §5 · FEATURES F-TAB · depends on: T-FE-001

### PostgreSQL plugin (M1–M2)

#### T-BE-010 — `mystique-plugin-postgres` connect + metadata
- **Priority:** P0 · **Tags:** `BE`
- **Description:** tokio-postgres/sqlx; test/connect; list schemas, tables, columns, indexes, FKs.
- **Related:** PRD V1 · ARCHITECTURE Phase 1 · depends on: T-BE-001

#### T-BE-011 — Postgres data page + read-only SQL execute
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Paginated `fetch_data_page`; `execute_query` with SELECT-only policy, max rows, timeout, history append.
- **Related:** PRD D-*, Q-* · depends on: T-BE-010, T-BE-003

#### T-BE-012 — Postgres Model graph (ER from FKs)
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Build ModelGraph from declared PK/FK only; no invented edges; confidence CONFIRMED.
- **Related:** PRD V-1–V-6 · FEATURES F-REL · depends on: T-BE-010

#### T-FE-010 — Explorer tree (relational)
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Schemas → tables/views → columns; search; open data tab.
- **Related:** FEATURES F-CONN · UI_UX §7.4 · depends on: T-BE-010, T-FE-003

#### T-FE-011 — Data grid + pagination
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Dense grid, null rendering, page size, sticky header; shared results component.
- **Related:** FEATURES F-ROW partial · PRD D-* · depends on: T-BE-011

#### T-FE-012 — SQL query editor
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Editor + Run (⌘/Ctrl+Enter) + results + error panel; read-only connection hides writes.
- **Related:** FEATURES F-SQ · UI_UX §7.5 · depends on: T-BE-011

#### T-FE-013 — Model view canvas (ER)
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Svelte Flow (or equivalent) nodes/edges; zoom/pan/search; click table → data; click edge → detail.
- **Related:** FEATURES F-REL-1–5 · PRD Model view · depends on: T-BE-012

#### T-FE-014 — Export results CSV/JSON
- **Priority:** P0 · **Tags:** `FE` `BE`
- **Description:** Export current grid/result set.
- **Related:** PRD D-5 · depends on: T-FE-011

### SQLite engine (M3)

#### T-BE-020 — `mystique-plugin-sqlite` (user DB files)
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Open path from profile; same relational SPI as Postgres; ER from SQLite FK pragma when present.
- **Related:** PRD V1 SQLite · depends on: T-BE-001, T-FE-001

#### T-FE-020 — SQLite form polish + file picker
- **Priority:** P1 · **Tags:** `FE`
- **Description:** Native/file dialog for path; validate extension; reuse relational workspace.
- **Related:** Engine picker sqlite · depends on: T-BE-020

### MongoDB (Next)

#### T-BE-030 — `mystique-plugin-mongodb`
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Connect, list databases/collections, document preview page, find queries, inferred schema.
- **Related:** PRD Next · ARCHITECTURE §7.3 · depends on: T-BE-001

#### T-BE-031 — Mongo Model view builder (inferred refs)
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Collection map + inferred edges labeled with confidence; never claim FK.
- **Related:** PRD Model view non-relational · depends on: T-BE-030

#### T-FE-030 — Document list + JSON pretty viewer
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Pretty JSON, tree/text toggle, edit modal pattern (writes later).
- **Related:** FEATURES F-JSON · depends on: T-BE-030

#### T-FE-031 — Mongo Model view UI
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Same canvas shell as ER; different node chrome for collections.
- **Related:** F-REL-9 · depends on: T-BE-031, T-FE-013

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

#### T-BE-040 — Integration tests (Postgres Testcontainers)
- **Priority:** P1 · **Tags:** `BE`
- **Description:** Smoke connect, metadata, ER edges golden path, policy rejects non-SELECT.
- **Related:** PRD §14 · depends on: T-BE-010–012

#### T-FE-050 — E2E / typecheck CI for UI
- **Priority:** P1 · **Tags:** `FE`
- **Description:** `yarn check` in CI; optional Playwright smoke for engine picker + form.
- **Related:** depends on: T-FE-001

#### T-BE-041 — Desktop packaging (Tauri release)
- **Priority:** P1 · **Tags:** `BE` `FE`
- **Description:** Signed/unsigned installers for target OS; native Rust binary, no JVM.
- **Related:** PRD packaging · depends on: M1 loop

### Later / P2+

#### T-BE-050 — MySQL / MariaDB plugin
- **Priority:** P2 · **Tags:** `BE`
- **Description:** Second network SQL engine via same SPI.
- **Related:** PRD Next · depends on: T-BE-001

#### T-BE-051 — AWS RDS connection presets + SSL/IAM
- **Priority:** P2 · **Tags:** `BE` `FE`
- **Description:** Connection form presets for managed Postgres/MySQL; optional Secrets Manager later.
- **Related:** PRD Next AWS · depends on: T-BE-010

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

1. **T-BE-001 → T-BE-002 → T-BE-003 → T-BE-004** (SPI base)  
2. **T-BE-010 → T-FE-010 → T-BE-011 → T-FE-011/012** (Postgres loop)  
3. **T-BE-012 → T-FE-013** (Model ER)  
4. **T-BE-020** (SQLite plugin) · **T-BE-030** (Mongo)  
5. Tabs / row panel / saved queries UI polish  

---

## Board snapshot

| Column | Count (approx.) |
| ------ | --------------- |
| Deployed | 6 |
| Progress | 0 |
| QA | 0 |
| TODO | 30+ |

Update this file when starting/finishing work (move the full ticket block between sections).
