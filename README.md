# mystique-db

Desktop app built with [Tauri 2](https://tauri.app/), [SvelteKit](https://kit.svelte.dev/), and TypeScript.

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

### 3. Rust

Install Rust via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Restart your shell, then verify:

```bash
rustc --version
cargo --version
```

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
├── src/                 # SvelteKit frontend
│   ├── routes/          # App pages
│   └── app.html
├── src-tauri/           # Rust / Tauri backend
│   ├── src/             # Rust source (commands, app entry)
│   ├── capabilities/    # Tauri security capabilities
│   ├── icons/           # App icons
│   ├── Cargo.toml
│   └── tauri.conf.json
├── static/              # Static assets
├── package.json
└── vite.config.js
```

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
