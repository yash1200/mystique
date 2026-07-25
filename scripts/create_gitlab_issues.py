#!/usr/bin/env python3
"""Create GitLab issues from MystiqueDB kanban tickets.

Usage:
  export GITLAB_TOKEN='glpat-...'   # needs api scope
  python3 scripts/create_gitlab_issues.py
"""
from __future__ import annotations

import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

GITLAB_URL = os.environ.get("GITLAB_URL", "https://nova.teachx.ai").rstrip("/")
PROJECT = os.environ.get("GITLAB_PROJECT", "trace-analysis/mystique-db")
TOKEN = os.environ.get("GITLAB_TOKEN") or os.environ.get("GL_TOKEN") or os.environ.get("PRIVATE_TOKEN")

TICKETS = [
    {"id": "T-BASE-001", "title": "Tauri + SvelteKit scaffold", "column": "deployed", "tags": ["FE", "BE", "P0"],
     "description": "Desktop shell with Vite/SvelteKit SPA, Tauri 2 window, icons, dev/build scripts.",
     "related": "README · Initial commit"},
    {"id": "T-BASE-002", "title": "Design system primitives", "column": "deployed", "tags": ["FE", "P0"],
     "description": "Semantic tokens, light/dark theme, UI primitives (Button, Field, Card, …) per design language.",
     "related": "DESIGN_LANGUAGE.md · UI_UX_GUIDELINES.md"},
    {"id": "T-BASE-003", "title": "Local app SQLite + profile CRUD", "column": "deployed", "tags": ["BE", "P0"],
     "description": "mystique-core domain/storage/application; migrations; IPC for profiles, query library, history, settings.",
     "related": "SQLITE_SCHEMA.md · BACKEND_STRUCTURE.md"},
    {"id": "T-BASE-004", "title": "Home connection rail + detail/edit UI", "column": "deployed", "tags": ["FE", "P0"],
     "description": "Left rail list, connection detail, new/edit forms wired to profile APIs (localStorage FE fallback).",
     "related": "FEATURES F-CONN-* · home components"},
    {"id": "T-BASE-005", "title": "Product docs set", "column": "deployed", "tags": ["DOCS", "P0"],
     "description": "PRD, architecture, features, UI/UX, design language, component structure, schema docs in docs/.",
     "related": "docs/*"},
    {"id": "T-FE-001", "title": "Engine picker (Postgres / Mongo / SQLite)", "column": "deployed", "tags": ["FE", "P0"],
     "description": "First step of new connection: choose database engine with design-language cards. No live BE plugins yet.",
     "related": "PRD §5 · EnginePicker.svelte · NewConnectionPanel"},
    {"id": "T-DOCS-001", "title": "HTML/CSS Kanban board", "column": "deployed", "tags": ["DOCS", "FE", "P0"],
     "description": "Standalone docs/kanban.html board for progress tracking with FE/BE tags.",
     "related": "docs/kanban.html · docs/KANBAN.md"},
    {"id": "T-BE-001", "title": "Connector API crate (mystique-connector-api)", "column": "todo", "tags": ["BE", "P0"],
     "description": "Traits DatabaseConnector + ConnectorSession, capabilities, errors, normalized DTOs. No engine drivers yet.",
     "related": "PRD §9 · ARCHITECTURE §4"},
    {"id": "T-BE-002", "title": "Connector registry + mock connector", "column": "todo", "tags": ["BE", "P0"],
     "description": "ConnectorRegistry loads plugins; mock connector proves SPI for tests and FE capability chrome.",
     "related": "ARCHITECTURE §4.4 · depends: T-BE-001"},
    {"id": "T-BE-003", "title": "Session manager + query policy stubs", "column": "todo", "tags": ["BE", "P0"],
     "description": "Connect/disconnect lifecycle; read-only SQL policy, timeout, max rows.",
     "related": "PRD Q-3/Q-4 · ARCHITECTURE §6 · depends: T-BE-001"},
    {"id": "T-BE-004", "title": "IPC surface for engines & sessions", "column": "todo", "tags": ["BE", "FE", "P0"],
     "description": "Tauri commands: list_engines, test_connection, connect, disconnect, get_capabilities.",
     "related": "ARCHITECTURE §10 · depends: T-BE-002, T-BE-003"},
    {"id": "T-BE-005", "title": "OS keychain for secrets", "column": "todo", "tags": ["BE", "P0"],
     "description": "Store passwords under secret_ref; never plaintext in SQLite.",
     "related": "PRD C-5 · depends: T-BASE-003"},
    {"id": "T-BE-006", "title": "Metadata / data / query / model core services", "column": "todo", "tags": ["BE", "P0"],
     "description": "MetadataService, DataService, QueryService, ModelGraphService, ExportService, AuditLog.",
     "related": "ARCHITECTURE §5–6 · depends: T-BE-001–004"},
    {"id": "T-FE-002", "title": "Capability-driven workspace chrome", "column": "todo", "tags": ["FE", "P0"],
     "description": "Show/hide Explorer, Data, Query, Model from EngineCapabilities; table vs collection nouns.",
     "related": "PRD §5.3 · UI_UX §8.6 · depends: T-BE-004"},
    {"id": "T-FE-003", "title": "App workspace shell layout", "column": "todo", "tags": ["FE", "P0"],
     "description": "Connection tabs + workspace tabs + object tree + main pane + status bar.",
     "related": "UI_UX §5 · FEATURES F-TAB · depends: T-FE-001"},
    {"id": "T-BE-010", "title": "mystique-plugin-postgres connect + metadata", "column": "todo", "tags": ["BE", "P0"],
     "description": "tokio-postgres/sqlx; test/connect; list schemas, tables, columns, indexes, FKs.",
     "related": "PRD V1 · ARCHITECTURE Phase 1 · depends: T-BE-001"},
    {"id": "T-BE-011", "title": "Postgres data page + read-only SQL", "column": "todo", "tags": ["BE", "P0"],
     "description": "Paginated fetch_data_page; execute_query SELECT-only, max rows, timeout, history.",
     "related": "PRD D-*, Q-* · depends: T-BE-010, T-BE-003"},
    {"id": "T-BE-012", "title": "Postgres Model graph (ER from FKs)", "column": "todo", "tags": ["BE", "P0"],
     "description": "Build ModelGraph from declared PK/FK only; no invented edges.",
     "related": "PRD V-* · FEATURES F-REL · depends: T-BE-010"},
    {"id": "T-FE-010", "title": "Explorer tree (relational)", "column": "todo", "tags": ["FE", "P0"],
     "description": "Schemas → tables/views → columns; search; open data tab.",
     "related": "UI_UX §7.4 · depends: T-BE-010, T-FE-003"},
    {"id": "T-FE-011", "title": "Data grid + pagination", "column": "todo", "tags": ["FE", "P0"],
     "description": "Dense grid, null rendering, page size, sticky header.",
     "related": "PRD D-* · depends: T-BE-011"},
    {"id": "T-FE-012", "title": "SQL query editor", "column": "todo", "tags": ["FE", "P0"],
     "description": "Editor + Run (⌘/Ctrl+Enter) + results + error panel.",
     "related": "FEATURES F-SQ · UI_UX §7.5 · depends: T-BE-011"},
    {"id": "T-FE-013", "title": "Model view canvas (ER)", "column": "todo", "tags": ["FE", "P0"],
     "description": "Node/edge canvas; zoom/pan/search; click table → data; edge → detail.",
     "related": "FEATURES F-REL · depends: T-BE-012"},
    {"id": "T-FE-014", "title": "Export results CSV/JSON", "column": "todo", "tags": ["FE", "BE", "P0"],
     "description": "Export current grid/result set.",
     "related": "PRD D-5 · depends: T-FE-011"},
    {"id": "T-BE-020", "title": "mystique-plugin-sqlite (user DB files)", "column": "todo", "tags": ["BE", "P0"],
     "description": "Open path from profile; same relational SPI as Postgres; ER from FK pragma.",
     "related": "PRD V1 SQLite · depends: T-BE-001"},
    {"id": "T-FE-020", "title": "SQLite form polish + file picker", "column": "todo", "tags": ["FE", "P1"],
     "description": "Native/file dialog for path; validate extension.",
     "related": "depends: T-BE-020"},
    {"id": "T-BE-030", "title": "mystique-plugin-mongodb", "column": "todo", "tags": ["BE", "P0"],
     "description": "Connect, list DBs/collections, document preview, find queries, inferred schema.",
     "related": "PRD Next · depends: T-BE-001"},
    {"id": "T-BE-031", "title": "Mongo Model view builder (inferred refs)", "column": "todo", "tags": ["BE", "P0"],
     "description": "Collection map + inferred edges labeled with confidence.",
     "related": "PRD Model view · depends: T-BE-030"},
    {"id": "T-FE-030", "title": "Document list + JSON pretty viewer", "column": "todo", "tags": ["FE", "P0"],
     "description": "Pretty JSON, tree/text toggle, edit modal pattern.",
     "related": "FEATURES F-JSON · depends: T-BE-030"},
    {"id": "T-FE-031", "title": "Mongo Model view UI", "column": "todo", "tags": ["FE", "P0"],
     "description": "Same canvas shell as ER; collection node chrome.",
     "related": "F-REL-9 · depends: T-BE-031, T-FE-013"},
    {"id": "T-FE-040", "title": "Browser-like workspace tabs", "column": "todo", "tags": ["FE", "P0"],
     "description": "Unlimited tabs per connection; table/query/model; dirty indicator.",
     "related": "F-TAB-1–4 · depends: T-FE-003"},
    {"id": "T-FE-041", "title": "Row detail right panel", "column": "todo", "tags": ["FE", "P0"],
     "description": "Vertical field list; prev/next; JSON expand; resize/collapse.",
     "related": "F-ROW-1–6 · depends: T-FE-011"},
    {"id": "T-FE-042", "title": "Edit / insert modals (gated writes)", "column": "todo", "tags": ["FE", "BE", "P0"],
     "description": "Row edit/insert modal, confirm before write, respect read-only.",
     "related": "F-EDIT-* · depends: T-FE-011"},
    {"id": "T-FE-043", "title": "Saved queries UI (groups per connection)", "column": "todo", "tags": ["FE", "P0"],
     "description": "UI on existing query_library BE; open in tab; rename/move/delete.",
     "related": "F-SQ-* · depends: T-FE-012"},
    {"id": "T-FE-044", "title": "Query history UI", "column": "todo", "tags": ["FE", "P1"],
     "description": "Browse past runs; re-run; filter by connection.",
     "related": "depends: T-BASE-003, T-FE-012"},
    {"id": "T-FE-045", "title": "Collapse connection rail", "column": "todo", "tags": ["FE", "P1"],
     "description": "Icon-only sidebar mode.",
     "related": "F-CONN-5 · depends: T-BASE-004"},
    {"id": "T-BE-040", "title": "Integration tests (Postgres Testcontainers)", "column": "todo", "tags": ["BE", "P1"],
     "description": "Smoke connect, metadata, ER edges, policy rejects non-SELECT.",
     "related": "PRD §14 · depends: T-BE-010–012"},
    {"id": "T-FE-050", "title": "E2E / typecheck CI for UI", "column": "todo", "tags": ["FE", "P1"],
     "description": "svelte-check in CI; optional Playwright smoke for engine picker.",
     "related": "depends: T-FE-001"},
    {"id": "T-BE-041", "title": "Desktop packaging (Tauri release)", "column": "todo", "tags": ["BE", "FE", "P1"],
     "description": "Installers for target OS; native Rust binary.",
     "related": "PRD packaging"},
    {"id": "T-BE-050", "title": "MySQL / MariaDB plugin", "column": "todo", "tags": ["BE", "P2"],
     "description": "Second network SQL engine via same SPI.",
     "related": "PRD Next · depends: T-BE-001"},
    {"id": "T-BE-051", "title": "AWS RDS connection presets + SSL/IAM", "column": "todo", "tags": ["BE", "FE", "P2"],
     "description": "Presets for managed Postgres/MySQL; optional Secrets Manager.",
     "related": "PRD Next AWS · depends: T-BE-010"},
    {"id": "T-BE-052", "title": "Cassandra / Keyspaces plugin", "column": "todo", "tags": ["BE", "P2"],
     "description": "CQL + key-model Model view.",
     "related": "PRD Future · depends: T-BE-001"},
    {"id": "T-FE-060", "title": "Diagram export PNG/SVG", "column": "todo", "tags": ["FE", "P2"],
     "description": "Export Model view.",
     "related": "F-REL-8 · depends: T-FE-013"},
    {"id": "T-FE-061", "title": "Session restore tabs", "column": "todo", "tags": ["FE", "BE", "P1"],
     "description": "Restore open tabs after restart.",
     "related": "F-TAB-5 · depends: T-FE-040"},
    {"id": "T-PROD-001", "title": "Command palette", "column": "todo", "tags": ["FE", "P2"],
     "description": "Cmd/Ctrl+K jump connections, tables, commands.",
     "related": "UI_UX §8.2 · depends: T-FE-003"},
]


def api(method: str, path: str, data=None):
    url = GITLAB_URL + path
    body = None if data is None else json.dumps(data).encode()
    req = urllib.request.Request(
        url,
        data=body,
        method=method,
        headers={
            "PRIVATE-TOKEN": TOKEN,
            "Content-Type": "application/json",
            "Accept": "application/json",
            "User-Agent": "Mozilla/5.0 MystiqueDB-issue-import/1.0",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            raw = r.read().decode()
            return r.status, json.loads(raw) if raw else None
    except urllib.error.HTTPError as e:
        raw = e.read().decode()
        try:
            parsed = json.loads(raw)
        except Exception:
            parsed = raw
        raise RuntimeError(f"HTTP {e.code} {method} {path}: {parsed}") from e


def ensure_labels(project_id: int, labels: list[str]):
    colors = {
        "FE": "#5b8def", "BE": "#9b6de8", "DOCS": "#8b95a8",
        "P0": "#f07178", "P1": "#e6c07b", "P2": "#6b7690", "P3": "#6b7690",
        "kanban:todo": "#9aa6bd", "kanban:progress": "#5b8def",
        "kanban:qa": "#e6c07b", "kanban:deployed": "#7fd99a",
        "mystique": "#4ecdc4",
    }
    _, existing = api("GET", f"/api/v4/projects/{project_id}/labels?per_page=100")
    have = {l["name"] for l in (existing or [])}
    for name in labels:
        if name in have:
            continue
        try:
            api("POST", f"/api/v4/projects/{project_id}/labels",
                {"name": name, "color": colors.get(name, "#8b95a8")})
            print(f"  label created: {name}")
        except Exception as e:
            print(f"  label skip {name}: {e}")


def list_existing_issues(project_id: int) -> dict:
    out = {}
    page = 1
    while True:
        _, items = api("GET", f"/api/v4/projects/{project_id}/issues?per_page=100&page={page}&state=all")
        if not items:
            break
        for it in items:
            title = it.get("title") or ""
            if title.startswith("[") and "]" in title:
                tid = title[1:title.index("]")]
                out[tid] = it
        if len(items) < 100:
            break
        page += 1
    return out


def issue_body(t: dict) -> str:
    return f"""## {t['id']}

**Kanban column:** `{t['column']}`  
**Priority / tags:** {', '.join(t['tags'])}

### Description
{t['description']}

### Related
{t.get('related') or '—'}

---
_Imported from MystiqueDB kanban (`docs/kanban.html` / `docs/KANBAN.md`)._
"""


def main():
    global TOKEN
    if not TOKEN:
        print(
            "ERROR: Set GITLAB_TOKEN (Personal Access Token with `api` scope).\n"
            "Create: https://nova.teachx.ai/-/user_settings/personal_access_tokens\n"
            "  export GITLAB_TOKEN='glpat-...'\n"
            "  python3 scripts/create_gitlab_issues.py",
            file=sys.stderr,
        )
        sys.exit(1)

    _, user = api("GET", "/api/v4/user")
    print(f"Authenticated as @{user.get('username')} (id={user.get('id')})")

    proj_path = urllib.parse.quote(PROJECT, safe="")
    _, project = api("GET", f"/api/v4/projects/{proj_path}")
    project_id = project["id"]
    print(f"Project: {project.get('path_with_namespace')} (id={project_id})")
    print(f"Issues: {project.get('web_url')}/-/issues")

    all_labels = sorted({*{tag for t in TICKETS for tag in t["tags"]}, "mystique",
                         "kanban:todo", "kanban:progress", "kanban:qa", "kanban:deployed"})
    print("Ensuring labels…")
    ensure_labels(project_id, all_labels)

    existing = list_existing_issues(project_id)
    print(f"Existing imported-style issues: {len(existing)}")

    created = updated = closed = 0
    for t in TICKETS:
        labels = ["mystique", f"kanban:{t['column']}", *t["tags"]]
        title = f"[{t['id']}] {t['title']}"
        description = issue_body(t)
        want_closed = t["column"] == "deployed"

        if t["id"] in existing:
            issue = existing[t["id"]]
            iid = issue["iid"]
            payload = {"title": title, "description": description, "labels": ",".join(labels)}
            if want_closed and issue.get("state") == "opened":
                payload["state_event"] = "close"
            elif not want_closed and issue.get("state") == "closed":
                payload["state_event"] = "reopen"
            api("PUT", f"/api/v4/projects/{project_id}/issues/{iid}", payload)
            updated += 1
            if payload.get("state_event") == "close":
                closed += 1
            print(f"  updated #{iid} {t['id']}")
            continue

        _, issue = api("POST", f"/api/v4/projects/{project_id}/issues",
                       {"title": title, "description": description, "labels": ",".join(labels)})
        iid = issue["iid"]
        created += 1
        if want_closed:
            api("PUT", f"/api/v4/projects/{project_id}/issues/{iid}", {"state_event": "close"})
            closed += 1
            print(f"  created+closed #{iid} {t['id']}")
        else:
            print(f"  created open #{iid} {t['id']}")
        time.sleep(0.15)

    print(f"\nDone. created={created} updated={updated} closed={closed}")
    print(f"Filter: {project.get('web_url')}/-/issues?label_name[]=mystique")


if __name__ == "__main__":
    main()
