# MystiqueDB — Feature List

> **Status:** Product feature catalog (user priorities)  
> **Related:** [`PRD.md`](./PRD.md) · [`ARCHITECTURE_AND_FEATURES.md`](./ARCHITECTURE_AND_FEATURES.md) · [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md)  
> **Last updated:** 2026-07-25

This document lists **what MystiqueDB should offer**, with emphasis on the features called out as product priorities. It is the feature checklist for design and implementation—not a technical SPI deep-dive.

**Legend**

| Tag | Meaning |
| --- | ------- |
| **P0** | Core product experience — ship early |
| **P1** | Strong V1 / soon after core loop |
| **P2** | Next wave after solid studio basics |
| **P3** | Later / polish |

---

## 1. Priority features (product focus)

These are the **prominent** capabilities MystiqueDB is built around.

### 1.1 Multiple connections — left rail

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-CONN-1** | Left connection panel | **P0** | Vertical left rail listing all saved / open database connections (not limited by free-tier tab caps). |
| **F-CONN-2** | Switch connection without new window | **P0** | Click a connection in the left rail to focus its workspace; multiple DBs live in one window. |
| **F-CONN-3** | Connection identity | **P0** | Each connection shows name, engine icon, status (connected/error), and **user-chosen color** (see UI guidelines). |
| **F-CONN-4** | Connection actions | **P0** | Add, edit, delete, duplicate, reconnect, disconnect from the left rail / context menu. |
| **F-CONN-5** | Collapse left rail | **P1** | Icon-only mode to maximize data space. |

```
┌────────────┬─────────────────────────────────────┐
│ ● local-pg │  [ users ] [ orders ] [ Query 1 ]   │
│ ● staging  │                                     │
│ ○ prod-mg  │         main workspace              │
│  + Add     │                                     │
└────────────┴─────────────────────────────────────┘
     ↑ left rail: multiple connections
```

---

### 1.2 Browser-like tabs — multiple tables & queries

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-TAB-1** | Browser-style workspace tabs | **P0** | Open many tables, views, queries, and tools as tabs (Chrome-like): reorder, close, dirty indicator. |
| **F-TAB-2** | Unlimited tabs per connection | **P0** | No artificial 2-tab limit; scroll / overflow menu when many tabs open. |
| **F-TAB-3** | Tab types | **P0** | At least: **Table data**, **Query editor**, later **Model**, **Import**. |
| **F-TAB-4** | Tab context | **P0** | Tabs belong to the active connection; switching left-rail connection shows that connection’s tab set. |
| **F-TAB-5** | Restore session | **P1** | Optionally restore open tabs after restart. |
| **F-TAB-6** | Split / pin tab | **P2** | Pin important tabs; optional split view later. |

---

### 1.3 Edit in modals

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-EDIT-1** | Row edit modal | **P0** | Edit a row in a focused modal/sheet (not only cramped in-grid cells)—clear fields, types, validation. |
| **F-EDIT-2** | Insert row modal | **P0** | Add a new row via the same modal pattern (empty form + defaults). |
| **F-EDIT-3** | Confirm before write | **P0** | Review changes (and generated SQL/ops when applicable) before apply. |
| **F-EDIT-4** | Delete with confirm | **P0** | Delete row/object only after explicit confirmation. |
| **F-EDIT-5** | Connection / settings modals | **P0** | Create/edit connection, import wizard steps, and settings use modal or side-sheet patterns consistently. |
| **F-EDIT-6** | Read-only mode | **P0** | When connection is read-only, hide/disable edit modals and show why. |

---

### 1.4 Pretty JSON edit

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-JSON-1** | Pretty-print JSON | **P0** | Format JSON/JSONB cell values and Mongo-style documents with indentation and syntax highlight. |
| **F-JSON-2** | Dual mode: text + tree | **P0** | Toggle **formatted text** editor and **tree** view (expand/collapse keys). |
| **F-JSON-3** | Validate on save | **P0** | Parse errors block save; show line/path of invalid JSON. |
| **F-JSON-4** | Edit in modal / side panel | **P0** | Large JSON opens in modal or right inspector—not a single-line cell. |
| **F-JSON-5** | Path breadcrumb | **P1** | Show current path when navigating nested keys in tree mode. |
| **F-JSON-6** | Copy path / copy value | **P1** | Quick actions from tree nodes. |

---

### 1.5 Row detail — right-side column panel

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-ROW-1** | Right-side row inspector | **P0** | Selecting a row opens (or focuses) a **right panel** listing all columns as a vertical field list. |
| **F-ROW-2** | Field-per-row layout | **P0** | Each column name + type chip + value on its own row—ideal for wide tables. |
| **F-ROW-3** | Navigate prev/next row | **P0** | Move selection up/down without leaving the panel. |
| **F-ROW-4** | Edit from inspector | **P1** | Open edit modal or inline-edit from the right panel. |
| **F-ROW-5** | JSON fields expand in place | **P0** | JSON columns use pretty viewer inside the inspector. |
| **F-ROW-6** | Collapse / resize panel | **P0** | User can hide the panel or drag width; remember preference. |

```
┌──────────────────────────────┬──────────────────┐
│  data grid (selected row)    │  Row detail      │
│                              │  id     42       │
│                              │  email  a@b.c    │
│                              │  meta   {…}      │
│                              │  created …       │
└──────────────────────────────┴──────────────────┘
                                 ↑ right column panel
```

---

### 1.6 Database relations — pretty Model / ER view

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-REL-1** | Relations / Model view | **P0** | Visual map of tables (and later collections) and how they connect—**DBeaver-class clarity**, modern styling. |
| **F-REL-2** | ER for relational DBs | **P0** | Tables as nodes; **declared FKs** as edges; PK/UK badges; cardinality when known. No invented FKs. |
| **F-REL-3** | Zoom, pan, search | **P0** | Navigate medium schemas (tens of tables) comfortably. |
| **F-REL-4** | Click table → open data tab | **P0** | Jump from diagram to browser tab for that table. |
| **F-REL-5** | Click edge → relationship detail | **P0** | Show FK columns, on-delete/update if available. |
| **F-REL-6** | Filter / focus | **P1** | Filter by schema, hide isolated tables, “focus neighbors” of selected table. |
| **F-REL-7** | Refresh from live metadata | **P0** | Rebuild graph from DB, not a stale file. |
| **F-REL-8** | Export diagram | **P2** | PNG/SVG export. |
| **F-REL-9** | Non-relational maps | **P2** | Document/key models with inferred links labeled (when those engines ship). |

---

### 1.7 Custom queries — stored in groups per connection

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-SQ-1** | Save custom query | **P0** | Save editor contents with a name against the **current connection**. |
| **F-SQ-2** | Groups / folders | **P0** | Organize saved queries into **groups** (e.g. “Reporting”, “Debug”, “Migrations”). |
| **F-SQ-3** | Per-connection library | **P0** | Query libraries are **scoped to each connection** (or connection profile)—not a single global dump that mixes prod and local. |
| **F-SQ-4** | Open saved query in tab | **P0** | One click opens a query tab with the SQL/MQL loaded. |
| **F-SQ-5** | Rename / move / delete | **P0** | Manage queries and groups from a sidebar section or modal. |
| **F-SQ-6** | Import / export query packs | **P2** | Share a group as JSON file. |
| **F-SQ-7** | Cross-connection copy | **P2** | Duplicate a query from one connection library to another (explicit action). |

```
Connection: local-pg
  Saved queries
  ├── 📁 Reporting
  │     ├── weekly_active_users
  │     └── revenue_by_day
  └── 📁 Debug
        └── find_orphan_orders
```

---

### 1.8 Past run queries (history)

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-HIST-1** | Query history | **P0** | Automatically record executed queries (text, time, duration, success/fail, row count). |
| **F-HIST-2** | History per connection | **P0** | Filter history by active connection (and optional global view). |
| **F-HIST-3** | Re-run from history | **P0** | Open any past query in a new tab or re-execute. |
| **F-HIST-4** | Search history | **P1** | Full-text search over past SQL/MQL. |
| **F-HIST-5** | Pin / promote to saved | **P1** | One action: history item → saved query (pick group). |
| **F-HIST-6** | Retention limits | **P1** | Cap history size or age; user-clearable. |
| **F-HIST-7** | Privacy | **P0** | Do not log secrets; optional “disable history” for a connection. |

---

### 1.9 Full table-related data & metadata

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-META-1** | Table data browser | **P0** | Browse all rows via pagination / virtual scroll (with safety caps configurable). |
| **F-META-2** | Columns panel | **P0** | Name, type, nullable, default, PK/FK flags. |
| **F-META-3** | Indexes | **P0** | List indexes and included columns. |
| **F-META-4** | Constraints | **P0** | PK, FK, unique, check—where the engine exposes them. |
| **F-META-5** | Triggers / rules | **P1** | Show when available (e.g. Postgres). |
| **F-META-6** | Relations summary | **P0** | Incoming/outgoing FKs for the table (list + link to Model view). |
| **F-META-7** | Estimated row count / size | **P1** | From engine stats when available. |
| **F-META-8** | DDL / create script | **P1** | View `CREATE TABLE` (or equivalent) definition. |
| **F-META-9** | Related data navigation | **P1** | From a row, jump to parent/child rows via FK (where applicable). |

“Show all data related to tables” = **data + structure + relationships** in one workspace, not only a grid.

---

### 1.10 Download data — CSV / TSV

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-EXP-1** | Export current result | **P0** | Download grid / query result as **CSV** or **TSV**. |
| **F-EXP-2** | Export full table | **P0** | Export entire table (or filtered set), not only the current page. |
| **F-EXP-3** | Stream large exports | **P0** | Backend streams to disk so huge sets do not freeze the UI. |
| **F-EXP-4** | Export options | **P1** | Headers on/off, delimiter, null encoding, encoding UTF-8. |
| **F-EXP-5** | Export JSON | **P1** | Optional JSON/NDJSON for nested data. |
| **F-EXP-6** | Progress + cancel | **P1** | Visible progress for long exports. |

---

### 1.11 Insert data from CSV / TSV

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-IMP-1** | Import wizard | **P0** | Choose file → map columns → preview → insert into target table. |
| **F-IMP-2** | CSV and TSV | **P0** | Both formats supported. |
| **F-IMP-3** | Column mapping | **P0** | Map file columns → table columns; skip unmapped. |
| **F-IMP-4** | Preview first N rows | **P0** | Catch type/delimiter issues before write. |
| **F-IMP-5** | Batch insert | **P0** | Efficient batched inserts; report success/fail counts. |
| **F-IMP-6** | Error rows report | **P1** | Show which rows failed and why. |
| **F-IMP-7** | Transactional import | **P1** | All-or-nothing option where the engine allows. |
| **F-IMP-8** | Guardrails | **P0** | Disabled in read-only connections; confirm target table name. |

---

### 1.12 AI query generation (local + cloud keys)

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-AI-1** | Prompt → query | **P1** | User describes intent in natural language; app proposes SQL (or engine query) in the editor. |
| **F-AI-2** | Schema-aware context | **P1** | Send relevant table/column metadata (and optional relations) so prompts are accurate. |
| **F-AI-3** | Local models | **P1** | Support local OpenAI-compatible endpoints (e.g. Ollama, LM Studio) via base URL. |
| **F-AI-4** | Custom API keys | **P1** | User-provided keys for **OpenAI**, **Anthropic**, **Gemini**, **Grok (xAI)**—stored securely, never in plain config logs. |
| **F-AI-5** | Provider picker | **P1** | Per-app or per-connection choice of provider/model. |
| **F-AI-6** | Review before run | **P0** | AI output is **draft only**; user must review and explicitly run (no auto-exec of generated queries). |
| **F-AI-7** | Explain / fix query | **P2** | Explain selected SQL; suggest fix from error message. |
| **F-AI-8** | Safety | **P0** | Prefer read-only framing; warn on destructive statements; respect connection read-only mode. |
| **F-AI-9** | Opt-in | **P0** | AI features off until user configures a provider; no silent network calls. |

| Provider | Mode |
| -------- | ---- |
| Local (Ollama / OpenAI-compatible) | Base URL + model name |
| OpenAI | API key + model |
| Anthropic | API key + model |
| Google Gemini | API key + model |
| xAI Grok | API key + model |

---

### 1.13 Filters on each column

| ID | Feature | Priority | Description |
| -- | ------- | -------- | ----------- |
| **F-FILT-1** | Per-column filter UI | **P0** | Every grid column has a filter control (header menu or under-header row). |
| **F-FILT-2** | Operators by type | **P0** | e.g. equals, contains, starts with, &gt;, &lt;, between, is null / is not null. |
| **F-FILT-3** | Multi-column combine | **P0** | Filters AND together (OR advanced later). |
| **F-FILT-4** | Server-side when possible | **P0** | Push filters into SQL/query so pagination stays correct. |
| **F-FILT-5** | Clear one / clear all | **P0** | Easy reset. |
| **F-FILT-6** | Filter chip bar | **P1** | Active filters shown as removable chips above the grid. |
| **F-FILT-7** | Distinct value picker | **P1** | For low-cardinality columns, pick from sample distinct values. |
| **F-FILT-8** | Persist filters per table tab | **P2** | Remember filters while the tab stays open (optional save). |

---

## 2. Supporting features (complete studio)

Still required for a coherent app; lower “signature” focus than §1.

| ID | Feature | Priority |
| -- | ------- | -------- |
| **F-SHELL-1** | Dark / light / system theme | P0 |
| **F-SHELL-2** | Connection color markers | P0 |
| **F-SHELL-3** | Keyboard shortcuts (run, new tab, close tab) | P0 |
| **F-SHELL-4** | Object explorer tree (schemas → tables → columns) | P0 |
| **F-SHELL-5** | SQL autocomplete (keywords + metadata) | P1 |
| **F-SHELL-6** | Query cancel | P1 |
| **F-SHELL-7** | Statement timeout & max rows | P0 |
| **F-SHELL-8** | Copy cell / row / column | P1 |
| **F-SHELL-9** | Null rendering | P0 |
| **F-SHELL-10** | Engine picker on Home | P0 |
| **F-SHELL-11** | Plugin-based engines (Postgres first) | P0 |
| **F-SHELL-12** | Secure credential storage | P0 |
| **F-SHELL-13** | Human-readable connection errors | P0 |
| **F-SHELL-14** | Read-only connection toggle | P0 |

---

## 3. Feature map by UI region

| UI region | Features |
| --------- | -------- |
| **Left rail** | Multiple connections, colors, status, add/edit |
| **Top tab bar** | Browser-like tabs for tables, queries, model, import |
| **Center grid** | Table data, column filters, selection |
| **Right panel** | Row detail (column view of one row), JSON pretty |
| **Modals / sheets** | Edit row, insert row, connection form, import, AI prompt |
| **Model / ER canvas** | Pretty relations view (DBeaver-like) |
| **Saved queries pane** | Groups per connection |
| **History pane** | Past run queries |
| **Table info** | Columns, indexes, constraints, DDL, relations |
| **Toolbar** | Run, export CSV/TSV, import CSV/TSV, AI generate, filter clear |

---

## 4. Suggested delivery waves

| Wave | Ship |
| ---- | ---- |
| **W1 — Navigate** | Left connections, browser tabs, explorer, table data, column filters (basic), right row panel |
| **W2 — Structure** | Relations/ER view, table metadata (indexes, FKs, DDL) |
| **W3 — Query library** | Editor, history, saved queries + groups per connection, export CSV/TSV |
| **W4 — Write path** | Edit/insert modals, pretty JSON edit, CSV/TSV import |
| **W5 — AI** | Prompt → query with local models + OpenAI / Anthropic / Gemini / Grok keys |

Exact engine order (e.g. Postgres first) remains in the PRD/architecture docs; **this list is feature-centric**.

---

## 5. Priority checklist (prominent only)

Use this as a wall checklist:

- [ ] **Left rail** for multiple connections (color + status)
- [ ] **Browser-like tabs** for multiple tables/queries
- [ ] **Edit in modals** (row insert/update + confirm)
- [ ] **Pretty JSON edit** (format + tree + validate)
- [ ] **Right-side row detail** (all columns for selected row)
- [ ] **Pretty relations / ER view** (DBeaver-like)
- [ ] **Saved queries in groups** (per connection)
- [ ] **Past run query history**
- [ ] **Full table-related data** (data + columns + indexes + constraints + relations)
- [ ] **Download CSV/TSV** (page + full result/table, streamed)
- [ ] **Insert from CSV/TSV** (wizard + mapping + preview)
- [ ] **AI query from prompt** (local models + OpenAI / Anthropic / Gemini / Grok API keys)
- [ ] **Per-column filters** (typed operators, multi-column)

---

## 6. Out of scope for this list (see PRD)

Not expanded here unless promoted later:

- Real-time multi-user collaboration  
- Full migration/DDL designer IDE  
- Mobile apps  
- Guaranteeing 20+ engines at launch  

---

## 7. Document history

| Version | Date | Notes |
| ------- | ---- | ----- |
| 1.0 | 2026-07-25 | Initial feature list from product priorities |

---

*MystiqueDB features: many connections, many tabs, clear structure, safe edits, and AI that drafts—never blindly runs.*
