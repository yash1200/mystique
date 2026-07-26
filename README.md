# mystique-db

Desktop app built with [Tauri 2](https://tauri.app/), [SvelteKit](https://kit.svelte.dev/), and TypeScript.

| | |
| --- | --- |
| **License** | [MIT](./LICENSE) |
| **Changelog** | [CHANGELOG.md](./CHANGELOG.md) |
| **Contributing** | [CONTRIBUTING.md](./CONTRIBUTING.md) |
| **Wiki** | [docs/wiki/](./docs/wiki/) (GitLab Wiki–friendly pages) |

## Recommended IDE setup

[VS Code](https://code.visualstudio.com/) with:

- [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode)
- [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Prerequisites

Install the following before cloning the repo.

### 1. Node.js

Node.js **18+** is required (LTS recommended). Check your version:

```bash
node -v
```

Install via [nodejs.org](https://nodejs.org/) or a version manager such as [nvm](https://github.com/nvm-sh/nvm) / [fnm](https://github.com/Schniz/fnm).

### 2. Yarn

This project uses **Yarn Classic** (`1.x`):

```bash
npm install -g yarn
yarn -v
```

### 3. Rust (required for `tauri dev`)

Install Rust via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

Verify (must work in the same terminal you run npm from):

```bash
rustc --version
cargo --version
echo "$PATH" | tr ':' '\n' | grep cargo   # should show ~/.cargo/bin
```

`npm run tauri` uses `scripts/tauri.sh`, which loads `~/.cargo/env` automatically so npm scripts find `cargo` even if your shell PATH is incomplete.

### 4. Platform dependencies (Tauri)

Tauri needs native system libraries. Install them for your OS:

#### macOS

- [Xcode Command Line Tools](https://developer.apple.com/xcode/resources/):

```bash
xcode-select --install
```

#### Linux (Debian/Ubuntu)

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

#### Windows

- [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (usually preinstalled on Windows 10/11)

See the full list in the [Tauri prerequisites docs](https://tauri.app/start/prerequisites/).

## Setup

```bash
git clone <repo-url>
cd mystique-db
yarn install
```

This installs frontend dependencies. Rust crates are fetched automatically on the first `tauri` build/dev run.

## Development

Start the full Tauri app (SvelteKit frontend + Rust backend):

```bash
yarn tauri dev
```

This:

1. Runs `yarn dev` (Vite on [http://localhost:1420](http://localhost:1420))
2. Compiles the Rust backend
3. Opens the native desktop window with hot reload

### Frontend only

To work on the UI without the native shell:

```bash
yarn dev
```

Then open [http://localhost:1420](http://localhost:1420) in a browser. Tauri APIs will not be available in this mode.

### Type checking

```bash
yarn check
# or watch mode:
yarn check:watch
```

## Build

Create a production desktop build:

```bash
yarn tauri build
```

Bundles are written under `src-tauri/target/release/bundle/` (format depends on OS: `.dmg` / `.app` on macOS, `.deb` / `.AppImage` on Linux, `.msi` / `.exe` on Windows).

## Project structure

```
mystique-db/
├── crates/
│   ├── mystique-connector-api/   # SPI (traits, DTOs, pool types) — no drivers
│   ├── mystique-core/            # domain, local app DB, ConnectionManager
│   └── plugins/
│       ├── mystique-plugin-postgres/  # deadpool pool
│       ├── mystique-plugin-sqlite/    # r2d2 pool
│       └── mystique-plugin-mongodb/   # driver client pool
├── docs/                  # PRD, architecture, design language, kanban.html
├── src/                   # SvelteKit frontend
├── src-tauri/             # Tauri shell + IPC API
├── package.json
└── Cargo.toml
```

**Engine pool API (Tauri invoke):** `list_engines`, `test_engine_connection`, `open_engine_session`, `close_engine_session`, `ping_engine_session`, `engine_session_pool_status`, `list_engine_sessions`. See `docs/BACKEND_STRUCTURE.md`.

Product docs: [`docs/PRD.md`](docs/PRD.md) · kanban: **[`docs/kanban.html`](docs/kanban.html)** · wiki: [`docs/wiki/Home.md`](docs/wiki/Home.md)

**V1 engines (product):** PostgreSQL · MongoDB · MySQL — each with its own **query policy** (read default, write opt-in, DELETE off). Home UI may still list SQLite until the MySQL plugin lands; see `docs/PRD.md` and `docs/kanban.html`.

## Useful scripts

| Command            | Description                                      |
| ------------------ | ------------------------------------------------ |
| `yarn dev`         | Vite dev server (frontend only)                  |
| `yarn build`       | Build static frontend (`build/`)                 |
| `yarn preview`     | Preview production frontend build                |
| `yarn check`       | Svelte / TypeScript type check                   |
| `yarn tauri dev`   | Full desktop app in development mode             |
| `yarn tauri build` | Production desktop bundle                        |

## Troubleshooting

- **Port 1420 already in use** — stop any process on that port, or change `server.port` in `vite.config.js` and `build.devUrl` in `src-tauri/tauri.conf.json` together.
- **Rust compile errors on first run** — ensure platform dependencies (step 4) are installed; the first compile can take several minutes.
- **`yarn` not found** — install Yarn globally with `npm install -g yarn`, or use `corepack enable` if you prefer Corepack.
