# Mystique

<img src="static/brand/mystique-mark.svg" width="48" height="48" alt="Mystique" />

Desktop app built with [Tauri 2](https://tauri.app/), [SvelteKit](https://kit.svelte.dev/), and TypeScript.

| | |
| --- | --- |
| **License** | [MIT](./LICENSE) |
| **Changelog** | [CHANGELOG.md](./CHANGELOG.md) |
| **Contributing** | [CONTRIBUTING.md](./CONTRIBUTING.md) |

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

Install Rust via [rustup](https://rustup.rs/). The repo `rust-toolchain.toml` pins **stable** and the Apple Silicon + Intel macOS targets (`aarch64-apple-darwin`, `x86_64-apple-darwin`); rustup installs those stdlibs the first time you run Cargo in this tree.

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

`yarn tauri` / `npm run tauri` runs the official `@tauri-apps/cli`. Cargo must be on `PATH` in the same terminal (`~/.cargo/bin` or `%USERPROFILE%\.cargo\bin`).

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

- [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) (or Visual Studio with the “Desktop development with C++” workload)
- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (usually preinstalled on Windows 10/11)
- Rust via [rustup](https://rustup.rs/) for **Windows** (MSVC toolchain). After install, open a **new** PowerShell/terminal so `cargo` is on `PATH`.

You do **not** need WSL or Git Bash to run `yarn tauri dev`. Use PowerShell.

Verify in PowerShell:

```powershell
node -v
yarn -v
rustc --version
cargo --version
```

See the full list in the [Tauri prerequisites docs](https://tauri.app/start/prerequisites/).

## Setup

```bash
git clone <repo-url>
cd mystique
yarn install
```

This installs frontend dependencies. Rust crates are fetched automatically on the first `tauri` build/dev run.

## Development

Start the full Tauri app (SvelteKit frontend + Rust backend). Same command on macOS, Linux, and Windows:

```bash
yarn tauri dev
# equivalents:
yarn tauri:dev
# or
npx tauri dev
```

This:

1. Runs the Vite frontend (`beforeDevCommand` → `npm run dev` on [http://localhost:1420](http://localhost:1420))
2. Compiles the Rust backend (Cargo workspace → root `target/` and `src-tauri/`)
3. Opens the native desktop window with hot reload

First compile can take several minutes. Subsequent runs are much faster.

### Frontend only

To work on the UI without the native shell:

```bash
yarn dev
```

Then open [http://localhost:1420](http://localhost:1420) in a browser. Tauri APIs will not be available in this mode.

### UI scale (large monitors / Linux)

Design-language sizes stay proportional (rem). Default UI scale is **1×** (design-language base: 16px root, 13px body). Optional zoom:

| Shortcut | Action |
| -------- | ------ |
| `Ctrl`/`⌘` `+` | Zoom in |
| `Ctrl`/`⌘` `-` | Zoom out |
| `Ctrl`/`⌘` `0` | Reset to auto |

Preference is stored in `localStorage` (`mystique-ui-scale`).

### Type checking

```bash
yarn check
# or watch mode:
yarn check:watch
```

## Build

Create a production desktop build for the machine you are on ([Tauri DMG guide](https://v2.tauri.app/distribute/dmg/)):

```bash
yarn tauri build
```

On an Apple Silicon Mac that is only the `aarch64` bundle. The Intel std target is listed in `rust-toolchain.toml`, so rustup installs it automatically.

```bash
# Intel Mac (cross-compiled from Apple Silicon)
yarn tauri:build:x86

# Apple Silicon, written under target/aarch64-apple-darwin/ instead of target/release/
yarn tauri build -- --target aarch64-apple-darwin

# one fat .app / .dmg that runs on both CPUs
yarn tauri build -- --target universal-apple-darwin

# Windows (.msi / .exe) — run this on a Windows machine
yarn tauri:build:windows
```

Windows bundles need the MSVC toolchain and WebView2; Tauri does not produce them from macOS. On Windows, `yarn tauri build` is enough if you only ship x64 — `yarn tauri:build:windows` is the same target, explicit.

Bundles:

| Command | Output |
| --- | --- |
| `yarn tauri build` (Apple Silicon host) | `target/release/bundle/` — `*_aarch64.dmg` |
| `--target aarch64-apple-darwin` | `target/aarch64-apple-darwin/release/bundle/` |
| `yarn tauri:build:x86` | `target/x86_64-apple-darwin/release/bundle/` |
| `--target universal-apple-darwin` | `target/universal-apple-darwin/release/bundle/` |
| `yarn tauri:build:windows` | `target/x86_64-pc-windows-msvc/release/bundle/` |

Formats: `.dmg` / `.app` on macOS, `.deb` / `.AppImage` on Linux, `.msi` / `.exe` on Windows.

## Project structure

```
mystique/
├── crates/
│   ├── mystique-connector-api/   # SPI (traits, DTOs, pool types) — no drivers
│   ├── mystique-core/            # domain, local app DB, ConnectionManager, AgentTools
│   ├── mystique-mcp/             # localhost MCP HTTP server
│   ├── mystique-agent/           # in-app query agent (Rig)
│   └── plugins/
│       ├── mystique-plugin-postgres/  # deadpool pool
│       ├── mystique-plugin-sqlite/    # r2d2 pool
│       └── mystique-plugin-mongodb/   # driver client pool
├── docs/                  # Design language, schema, remaining feature specs
├── src/                   # SvelteKit frontend
├── src-tauri/             # Tauri shell + IPC API
├── package.json
└── Cargo.toml
```

**Engine pool API (Tauri invoke):** `list_engines`, `test_engine_connection`, `open_engine_session`, `close_engine_session`, `ping_engine_session`, `engine_session_pool_status`, `list_engine_sessions`. See `docs/BACKEND_STRUCTURE.md`.

Living docs: [`docs/DESIGN_LANGUAGE.md`](docs/DESIGN_LANGUAGE.md) · [`docs/BACKEND_STRUCTURE.md`](docs/BACKEND_STRUCTURE.md) · [`docs/SQLITE_SCHEMA.md`](docs/SQLITE_SCHEMA.md). Remaining feature spec: [`docs/QUERY_TAB_CHAT_AGENT.md`](docs/QUERY_TAB_CHAT_AGENT.md).

**Engines:** PostgreSQL · MongoDB · MySQL · SQLite — each with its own **query policy** (read default, write opt-in, DELETE off).

## Useful scripts

| Command            | Description                                      |
| ------------------ | ------------------------------------------------ |
| `yarn dev`         | Vite dev server (frontend only)                  |
| `yarn build`       | Build static frontend (`build/`)                 |
| `yarn preview`     | Preview production frontend build                |
| `yarn check`       | Svelte / TypeScript type check                   |
| `yarn tauri dev`   | Full desktop app in development mode             |
| `yarn tauri build` | Production desktop bundle (host architecture)    |
| `yarn tauri:build:x86` | Intel Mac bundle (`x86_64-apple-darwin`) |
| `yarn tauri:build:windows` | Windows x64 bundle (`x86_64-pc-windows-msvc`; run on Windows) |
| `yarn tauri build -- --target universal-apple-darwin` | One universal (fat) macOS binary |

## Troubleshooting

- **Port 1420 already in use** — stop any process on that port, or change `server.port` in `vite.config.js` and `build.devUrl` in `src-tauri/tauri.conf.json` together.
- **Rust compile errors on first run** — ensure platform dependencies (step 4) are installed; the first compile can take several minutes.
- **`Target x86_64-apple-darwin is not installed`** — rustup should pick this up from `rust-toolchain.toml`. If not: `rustup target add x86_64-apple-darwin`.
- **`yarn` not found** — install Yarn globally with `npm install -g yarn`, or use `corepack enable` if you prefer Corepack.
- **`EBUSY: resource busy or locked, watch ...\target\...\*.dll`** — Vite was watching Cargo build output under the workspace `target/` directory. `vite.config.js` ignores `**/target/**` (and `**/src-tauri/**`). Pull latest or ensure those `server.watch.ignored` entries are present, then restart `yarn tauri dev`.
- **`cargo not found` from yarn scripts** — ensure `%USERPROFILE%\.cargo\bin` (Windows) or `~/.cargo/bin` (Unix) is on PATH in the same terminal (open a new one after installing Rust).
- **Linux WebKit MESA/EGL spam or 1× scale on large monitors** — export `WEBKIT_DISABLE_DMABUF_RENDERER=1` (and `XCURSOR_SIZE=16` if the cursor is huge). On machines with no DRM GPU, also `LIBGL_ALWAYS_SOFTWARE=1`.
