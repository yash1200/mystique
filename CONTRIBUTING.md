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

Frontend: `src/lib/engines/<engine>/` with `index.ts` and `Fields.svelte`, then one `register()` in `engines/registry.ts`. Chrome reads `EngineCapabilities`, not `if (engine === '…')`.

A **product** (picker / `connections.engine`) can share a **runtime** crate. Add a row to `crates/mystique-connector-api/src/products.rs` **and** `src/lib/engines/products.ts` — do not copy a plugin. `ConnectorRegistry::get("mariadb")` returns the MySQL connector. Picker cards for those products are Phase 1.4.

## Git

Branches: `feat/…`, `fix/…`, `docs/…`, `chore/…` off `main`. Conventional commits. MRs go to `main` with how to test.

Do not commit passwords, API keys, or `~/.tauri/mystique.key`.

## License

Contributions are MIT, same as the repo.
