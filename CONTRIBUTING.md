# Contributing

Mystique is a Tauri 2 + Svelte 5 desktop app. Rust workspace in `crates/`, UI in `src/`, thin IPC shell in `src-tauri/`.

## Setup

Need Node 18+, pnpm, Rust (stable, via rustup), and the usual Tauri native deps for your OS. On Windows use PowerShell + MSVC; you do not need WSL.

```bash
pnpm install
pnpm tauri dev          # desktop window
pnpm check              # boundaries + svelte-check
cargo test              # from repo root
```

`rust-toolchain.toml` pulls `aarch64-apple-darwin` and `x86_64-apple-darwin`. First `tauri dev` is slow.

## CI

GitLab (`.gitlab-ci.yml`) runs on `main` and merge requests:

- `pnpm check` — import boundaries + `svelte-check`
- `cargo test --workspace --exclude mystique --locked` — Rust unit tests (no Tauri/GTK, no live DB)

Live plugin tests (`#[ignore]`, local Postgres/MySQL/Mongo) stay off the default job:

```bash
cargo test -- --ignored
```

`pnpm tauri:build` also runs `pnpm check` via `beforeBuildCommand`.

## Layout

```text
crates/
  mystique-connector-api/   SPI: connector, query policy, DTOs. No drivers.
  mystique-core/            App DB, profiles, sessions, AgentTools
  mystique-mcp/             Localhost MCP HTTP server + client installers
  mystique-agent/           In-app query agent
  plugins/mystique-plugin-{postgres,mysql,mongodb,sqlite}/
src-tauri/                  Window, menu, IPC → services
src/
  routes/                   Mounts only
  lib/
    core/                   UI primitives, IPC, contracts, util
    engines/                Per-database form, dialect, capabilities
    features/
      connections/          Home: profiles
      settings/             Appearance, workspace, AI, MCP
      shortcuts/
      toast/
      workspace/
        shell/              Panes, sessions, tabs
        catalog/ explorer/ grid/ query/ er/ metrics/ agent/
    app/                    Theme / density prefs
    styles/                 tokens.css
docs/                       Design language, backend, schema
scripts/check-boundaries.mjs
```

Imports go **down**: `routes → features → engines → core`. Features import each other only through that feature’s `index.ts`. `pnpm check` runs `scripts/check-boundaries.mjs` and fails on a deep import.

```ts
import { Button } from '$core/ui'
import { adapterFor } from '$engines'
import { DataGrid } from '$features/workspace/grid'
import { workspaceSession } from '../session.svelte' // same feature
```

Full rules: [`docs/COMPONENT_STRUCTURE.md`](docs/COMPONENT_STRUCTURE.md), [`docs/BACKEND_STRUCTURE.md`](docs/BACKEND_STRUCTURE.md).

## Where a change goes

| If you are changing…                               | Put it in                                                                                |
| -------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| A button, modal, input used in more than one place | `src/lib/core/ui`                                                                        |
| A helper with no product meaning                   | `src/lib/core/util`                                                                      |
| A type Rust also serializes                        | `src/lib/core/contracts` + the Rust DTO                                                  |
| Anything that differs per database                 | `src/lib/engines/<engine>` **and** `crates/plugins/mystique-plugin-<engine>`             |
| A screen or its state                              | `src/lib/features/…`                                                                     |
| IPC                                                | `src-tauri/src/ipc/` — no SQL, no drivers                                                |
| Query safety                                       | `EngineQueryPolicy` on the plugin. Default is read. Write is opt-in. `DELETE` is denied. |

UI uses tokens from [`docs/DESIGN_LANGUAGE.md`](docs/DESIGN_LANGUAGE.md). No `text-[13px]`, no hex in components.

## Adding an engine

Backend: new `mystique-plugin-<engine>` crate (`DatabaseConnector` + `ConnectorSession` + `EngineQueryPolicy` + metrics). Register both connector and policy in `ConnectorRegistry::bootstrap()`.

Frontend: `src/lib/engines/<engine>/` with `index.ts`, then one `register()` in `engines/registry.ts`. Connection forms are Host / File / URI shells in `engines/ui/` keyed by `connectionKind` — do not add `Fields.svelte` per engine. Chrome reads `EngineCapabilities`, not `if (engine === '…')`.

A **product** (picker / `connections.engine`) can share a **runtime** crate. Add a row to `crates/mystique-connector-api/src/products.rs` **and** `src/lib/engines/products.ts` with `onPicker: true` — do not copy a plugin or add `Fields.svelte`. `ConnectorRegistry::get("mariadb")` returns the MySQL connector. Home cards come from `engineOptions()` / `onPicker`, not from extra `register()` calls.

`supportDepth` (`operate` | `browse` | `connect`) is the overlay that hides ER/metrics/admin without engine-name branches. New products default to `operate` only if they really have that chrome.

SSH is **transport on the profile** (`ssh_config` + keychain `ssh_secret_ref`), not plugin_config. `ConnectionManager` opens a local forward; plugins still connect to `127.0.0.1`. Do not add SSH fields to a plugin crate. SQLite/file forms have no SSH UI.

SSL is **shared** (`TlsSettings` in connector-api): `sslMode` disable/prefer/require/verify-ca/verify-full plus optional CA and client cert paths. Plugins must not each invent a TLS builder. Host and URI forms show SSL extras; File/SQLite does not.

**Write guard (P3.1):** `environment` is a runtime check in `ConnectionManager`, not a badge. Production and staging: first write in a session must confirm (unlock is in-memory, dies on disconnect/quit). DROP/TRUNCATE need type-to-confirm of the object name. Local: no extra prompt beyond read-only. Agent/MCP never call unlock; production writes fail unless the session is already unlocked in-app.

**MCP policy (P3.2):** Global `mcp.policy` is `read-only` (default), `safe_write` (INSERT/UPDATE only), or `high_risk_write` (DDL/DROP still go through P3.1). `mcp.allowlist` is connection ids; empty means none, not all. The in-app agent uses the same policy object if write tools are added; it is not filtered by the allowlist. `AGENT_MAX_ROWS` is asserted below the driver `HARD_MAX_ROWS`.

**SQL analysis (P4.1–4.2):** `mystique-sql` owns statement split, AST write-risk, and identifier quoting. `EngineQueryPolicy` for SQL engines, the query tab, agent, and MCP all call it. DELETE is allowed in Write mode (docs that said “always denied” were wrong). A fifth SQL dialect is a `SqlDialect` variant, not a copied keyword list. Mongo keeps its own policy.

## Git

Branches: `feat/…`, `fix/…`, `docs/…`, `chore/…` off `main`. Conventional commits. MRs go to `main` with how to test.

Do not commit passwords, API keys, or `~/.tauri/mystique.key`.

## License

Contributions are MIT, same as the repo.
