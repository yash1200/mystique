# Development

## Prerequisites

- Node.js 18+  
- Yarn Classic (or compatible install via npm)  
- Rust (rustup)  
- Platform libraries for Tauri (see root [README](../../README.md))  

### Windows notes

- Install the **MSVC** Rust toolchain and [C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/), plus WebView2.  
- Use **PowerShell** or Windows Terminal. You do **not** need WSL or Git Bash for `yarn tauri dev`.  
- Tauri scripts run via Node: `scripts/tauri.mjs` (not bash). The old `scripts/tauri.sh` remains for manual Unix use only.  
- Confirm `node`, `yarn`, `cargo`, and `rustc` work in the same terminal before starting the app.

## Common commands

Same on macOS, Linux, and Windows:

```bash
yarn install
yarn dev              # frontend only (browser; no Tauri APIs)
yarn tauri dev        # full desktop app (preferred for native APIs)
yarn tauri:dev        # alias for tauri dev
yarn check            # Svelte / TypeScript
yarn tauri build      # production bundle
```

If the yarn script fails on an old checkout, you can still run:

```bash
npx tauri dev
```

## Layout

| Path | Role |
| ---- | ---- |
| `src/` | SvelteKit UI |
| `src-tauri/` | Tauri shell + IPC |
| `crates/` | Rust core, SPI, plugins |
| `target/` | Cargo workspace build output (do not hand-edit) |
| `scripts/tauri.mjs` | Cross-platform `yarn tauri` launcher |
| `docs/` | PRD, architecture, kanban |
| `docs/wiki/` | Wiki-style pages (this tree) |

## Tips

- Prefer `yarn tauri dev` when testing connections and native dialogs.  
- Frontend-only mode does not provide Tauri APIs.  
- Keep design tokens (`docs/DESIGN_LANGUAGE.md`) when touching UI.  
- New engines: plugin crate + **own** query policy + registry registration.  
- Vite ignores `**/target/**` and `**/src-tauri/**` for HMR so Windows does not crash with `EBUSY` while cargo rewrites DLLs (see GitLab [#69](https://nova.teachx.ai/trace-analysis/mystique-db/-/issues/69)).  

## Troubleshooting (dev)

| Symptom | Fix |
| ------- | --- |
| `bash` / WSL `execvpe(/bin/bash)` | Use current `scripts/tauri.mjs` scripts, or `npx tauri dev`. Do not depend on Windows `bash` → WSL. |
| Vite `EBUSY` watching `target\...\*.dll` | Ensure `vite.config.js` ignores `**/target/**`; restart dev. |
| `cargo not found` | Put `~/.cargo/bin` (or `%USERPROFILE%\.cargo\bin`) on PATH; open a new terminal after rustup. |
| Port 1420 in use | Stop the other Vite/Tauri process; see root README. |

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for GitLab issues and merge requests.
