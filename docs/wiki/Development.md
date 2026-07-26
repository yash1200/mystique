# Development

## Prerequisites

- Node.js 18+  
- Yarn Classic (or compatible install via npm)  
- Rust (rustup)  
- Platform libraries for Tauri (see root README)  

## Common commands

```bash
yarn install
yarn dev              # frontend only
yarn tauri dev        # full desktop app
yarn check            # Svelte / TypeScript
yarn tauri build      # production bundle
```

## Layout

| Path | Role |
| ---- | ---- |
| `src/` | SvelteKit UI |
| `src-tauri/` | Tauri shell + IPC |
| `crates/` | Rust core, SPI, plugins |
| `docs/` | PRD, architecture, kanban |
| `docs/wiki/` | Wiki-style pages (this tree) |

## Tips

- Prefer `yarn tauri dev` when testing connections and native dialogs.  
- Frontend-only mode does not provide Tauri APIs.  
- Keep design tokens (`docs/DESIGN_LANGUAGE.md`) when touching UI.  
- New engines: plugin crate + **own** query policy + registry registration.  

## Contributing

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for GitLab issues and merge requests.
