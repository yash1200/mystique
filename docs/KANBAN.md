# MystiqueDB — Project Kanban

> **Status:** Living board (update columns when tickets move)  
> **Visual board (open in browser):** [`kanban.html`](./kanban.html)  
> **Sources:** [`PRD.md`](./PRD.md) · [`ARCHITECTURE_AND_FEATURES.md`](./ARCHITECTURE_AND_FEATURES.md) · [`FEATURES.md`](./FEATURES.md) · [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md) · [`BACKEND_STRUCTURE.md`](./BACKEND_STRUCTURE.md)  
> **Last updated:** 2026-07-27  
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

### T-DEV-MONGO — Local Mongo docker seed (mystique 10k)
- **Priority:** P1 · **Tags:** `BE` `DOCS` · **Status:** Deployed
- **Description:** mongo:7 :27017; **admin/123** `authSource=admin`; seed DB **`mystique`** (~10k docs). Legacy `mystique_dev` may remain. See DEV_MONGO.md + `scripts/seed/mongo_seed_mystique.js`.

### T-DEV-MYSQL — Local MySQL seed docs
- **Priority:** P1 · **Tags:** `BE` `DOCS` · **Status:** Deployed
- **Description:** DEV_MYSQL.md + `scripts/seed/mysql_seed_mystique.sql`. Plugin landed (T-BE-050).

### T-BE-003 — Per-engine query policy + execute IPC
- **Priority:** P0 · **Tags:** `BE` · **Status:** Deployed
- **Description:** `EngineQueryPolicy` + `execute_engine_query`; SQL (PG/SQLite/MySQL) + Mongo find; read/write modes; DELETE denied; results on FE.

### T-FE-012 — Query space (editor + run + live results)
- **Priority:** P0 · **Tags:** `FE` · **Status:** Deployed
- **Description:** Live execute, Allow writes, policy, results table, undo/redo, history append, Mongo collection + selected DB.

### T-FE-QUERY-RESULTS — Results pagination + CSV/PDF
- **Priority:** P0 · **Tags:** `FE` · **Status:** Deployed
- **Description:** 25 rows/page on query results; CSV + PDF export (`exportTable.ts`). DataGrid default 25/page + export.

### T-FE-014 — Export results CSV/PDF
- **Priority:** P0 · **Tags:** `FE` · **Status:** Deployed
- **Description:** Query + grid export via save dialog. JSON export optional later.

### T-FE-LIVE-CATALOG — Live explorer catalog + session rebind
- **Priority:** P0 · **Tags:** `FE` `BE` · **Status:** Deployed
- **Description:** Mongo list DBs/collections; SQL (MySQL/PG/SQLite) live tables; MySQL `TABLE_ROWS` counts; session rebind; table click preview.

### T-BE-MONGO-SHELL — Mongo shell filter parser
- **Priority:** P0 · **Tags:** `BE` · **Status:** Deployed
- **Description:** `ObjectId` / `ISODate` / unquoted keys / `db.coll.find({…})` → BSON.

### T-FE-QUERY-HISTORY — History pane + re-open editor
- **Priority:** P1 · **Tags:** `FE` · **Status:** Deployed
- **Description:** History rail from BE; re-open body; all engines.

### T-FE-044 — Query history UI
- **Priority:** P1 · **Tags:** `FE` · **Status:** Deployed
- **Description:** List + re-open + optimistic append; badge count; table-preview logging. Optional later: search/filter, clear.

### T-BE-050 — `mystique-plugin-mysql` + SqlQueryPolicy
- **Priority:** P0 · **Tags:** `BE` `FE` · **Status:** Deployed
- **Description:** mysql_async pool; connect/test; execute; FE MySQL picker + MysqlFields; live catalog + table data. **Related:** GitLab #41 · DEV_MYSQL.md

### T-FE-011 — Data grid + pagination
- **Priority:** P0 · **Tags:** `FE` · **Status:** Deployed
- **Description:** Grid, filters, 25–200/page, CSV/PDF export.

### T-FE-LIVE-ER — Live ER / Model from connected database
- **Priority:** P0 · **Tags:** `FE` `BE` · **Status:** Deployed
- **Description:** Explorer **ER diagram** button; full schema map for selected DB (MySQL/PG FKs, SQLite, Mongo collections); read-only tools chrome; model workspace tab. **Related:** `liveEr.ts` · T-BE-012 (QA depth) · GitLab **#68** · commits `c127169`, `39f2bc3`

### T-FE-001 — Engine picker (Postgres / Mongo / MySQL / SQLite)
- **Priority:** P0 · **Tags:** `FE` · **Status:** Deployed
- **Description:** Engine cards for new connection including MySQL.

### T-FE-TEST — Test connection FE → BE
- **Priority:** P0 · **Tags:** `FE` `BE` · **Status:** Deployed
- **Description:** `test_engine_connection` shows latency + version; Connect opens pooled session.

---

## Progress

_(empty — start next ticket here)_

---

## QA _(implemented — verify / polish)_

### T-BE-030 — `mystique-plugin-mongodb` + MongoQueryPolicy
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Connect/pool, policy, find + shell filters, list DBs/collections, URI default DB. **Still open:** aggregate execute, formal MetadataService, inferred Model graph.
- **Related:** T-BE-MONGO-SHELL · T-FE-LIVE-CATALOG · GitLab #31

### T-BE-011 — Postgres SQL execute + policy
- **Priority:** P0 · **Tags:** `BE`
- **Description:** `execute_query` + SQL policy works. **Still open:** formal `fetch_data_page`, richer live explorer metadata.
- **Related:** GitLab #17

### T-FE-040 — Workspace tabs
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Query + table + model tabs. **Still open:** dirty indicators, overflow menu.
- **Related:** GitLab #26

### T-FE-041 — Row detail right panel
- **Priority:** P0 · **Tags:** `FE`
- **Description:** RowInspector basic fields. **Still open:** prev/next, JSON expand, resize.
- **Related:** GitLab #27

### T-BE-012 — Postgres / SQL Model graph (ER from FKs)
- **Priority:** P0 · **Tags:** `BE` `FE`
- **Description:** Live ER tab loads FK graph (MySQL/PG/SQLite). **Still open:** formal `ModelGraphService` SPI, auto-layout polish.
- **Related:** T-FE-LIVE-ER · GitLab #22

### T-FE-010 — Explorer tree (relational)
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Live tables listed for SQL engines. **Still open:** column/index tree depth, schema grouping UX.
- **Related:** GitLab #20

---

## TODO

### Foundation

#### T-BE-005 — OS keychain for secrets
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Store passwords under `secret_ref`; never plaintext in SQLite.
- **Related:** PRD C-5 · depends on: T-BASE-003 · GitLab #12

#### T-BE-006 — Metadata / data / query / model core services
- **Priority:** P0 · **Tags:** `BE`
- **Description:** MetadataService, DataService, QueryService, ModelGraphService, ExportService, AuditLog.
- **Related:** ARCHITECTURE §5–6 · GitLab #11

#### T-FE-002 — Capability-driven workspace chrome
- **Priority:** P0 · **Tags:** `FE`
- **Description:** Show/hide Explorer/Data/Query/Model from `EngineCapabilities`.
- **Related:** GitLab #15

### PostgreSQL (V1)

#### T-BE-010 — `mystique-plugin-postgres` connect + metadata
- **Priority:** P0 · **Tags:** `BE`
- **Description:** Full metadata: schemas, tables, columns, indexes, FKs (partial connect + live ER already exists).
- **Related:** GitLab #21

### MySQL (V1)

_(T-BE-050 **Deployed** — connect / list tables / data / live ER)_

### MongoDB (remaining)

#### T-BE-031 — Mongo Model view builder (inferred refs) · **P1** · GitLab #24
#### T-FE-030 — Document list + JSON pretty viewer · **P0** · GitLab #25
#### T-FE-031 — Mongo Model view UI · **P1** · GitLab #28

### SQLite (later)

#### T-BE-020 — User file SQLite + policy polish · **P2** · GitLab #30
#### T-FE-020 — SQLite file picker · **P2** · GitLab #29

### Product features

#### T-FE-042 — Edit / insert modals (gated writes) · **P0** · GitLab #33
#### T-FE-043 — Saved queries UI · **P0** · GitLab #32
#### T-FE-045 — Collapse connection rail · **P1** · GitLab #43
#### T-FE-061 — Session restore tabs · **P1** · GitLab #40
#### T-PROD-001 — Command palette · **P2** · GitLab #44

### Quality & packaging

#### T-BE-040 — Integration tests (Postgres + policy) · **P1** · GitLab #36
#### T-FE-050 — E2E / typecheck CI · **P1** · GitLab #34
#### T-BE-041 — Desktop packaging · **P1** · GitLab #35

### Later / P2+

#### T-BE-051 — AWS RDS presets · **P2** · GitLab #42
#### T-BE-052 — Cassandra / Keyspaces · **P2** · GitLab #38
#### T-FE-DESIGN — Align UI with design language · **P1** · GitLab #54
#### T-BE-MCP / T-FE-AGENT / T-MCP-* — AI agent & MCP · GitLab #64–#67

---

## Suggested near-term order

1. **QA polish** — Mongo aggregate, row inspector, tab dirty state  
2. **T-BE-005** — OS keychain  
3. **T-FE-043** — Saved queries UI  
4. **T-BE-010** — Deeper Postgres metadata (indexes, etc.)  
5. Formal ModelGraphService / JSON edit depth  

Canonical board data: **`docs/kanban.html`**. Keep this file aligned when tickets move.

---

## Board snapshot (2026-07-27)

| Column | Count |
| ------ | ----- |
| Deployed | ~41 |
| Progress | 0 |
| QA | ~6 |
| TODO | ~20 |

### Recent session highlights (→ Deployed / QA)

| Work | Ticket(s) | GitLab |
| ---- | --------- | ------ |
| Query policy SPI + live execute | T-BE-003, T-FE-012 | #3, #16 |
| Mongo shell filters + find | T-BE-MONGO-SHELL, T-BE-030 (QA) | #31 |
| Live explorer catalog (Mongo + SQL) | T-FE-LIVE-CATALOG | — |
| MySQL plugin + seeds | T-BE-050, T-DEV-MYSQL | #41 |
| History for all engines | T-FE-QUERY-HISTORY, T-FE-044 | #39 |
| Grid + export | T-FE-QUERY-RESULTS, T-FE-014, T-FE-011 | #19, #23 |
| Live ER from connected DB (read-only) | T-FE-LIVE-ER, T-BE-012 (QA) | #22 |
