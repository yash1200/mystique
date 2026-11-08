# Frontend structure

> Vertical feature slices for Mystique (Svelte 5 + Tauri).

The frontend is organised by **feature**, not by kind. A feature owns its
components, state, pure logic, types and IPC together, so changing one thing
means opening one folder.

## Zones

```text
src/
├── routes/                       # thin mounts — nothing else lives here
│   ├── +layout.svelte            # theme, ui-scale, keymap + command dispatch
│   ├── +page.svelte              # <ConnectionsPage />
│   └── workspace/+page.svelte    # <WorkspacePage />
└── lib/
    ├── core/                     # domain-free; imported by everyone, imports nobody
    │   ├── ui/                   # Button, Field, Modal, Toast… (the design system)
    │   ├── ipc/                  # isTauri + the typed Tauri command surface
    │   ├── commands/             # command registry: keys, focus scopes, native-menu dispatch
    │   ├── contracts/            # DTOs mirroring mystique-connector-api + app-DB rows
    │   └── util/                 # cn, motion, ui-scale, files, clipboard, time, highlight
    │
    ├── engines/                  # ← the seam that scales to new databases
    │   ├── types.ts              # EngineAdapter
    │   ├── registry.ts           # id → adapter, aliases, fallback
    │   ├── capabilities.ts       # pure helpers over EngineCapabilities
    │   ├── sql/                  # shared SQL formatter + completion schema
    │   └── postgres/ mysql/ mongodb/ sqlite/
    │
    ├── features/                 # vertical slices
    │   ├── connections/          # home screen: profiles, picker, create/edit form
    │   ├── settings/             # appearance, workspace, AI, MCP
    │   ├── shortcuts/            # keyboard reference
    │   └── workspace/
    │       ├── session/          # kernel: session + studio stores, tabs, drafts, write guard (no UI)
    │       ├── shell/            # composition: panes, sessions rail, tabs, modals
    │       ├── catalog/          # live catalog, labels, icons
    │       ├── explorer/         # object tree, structure panel
    │       ├── grid/             # data grid, filters, import/export
    │       ├── query/            # editor, results, saved queries, history
    │       ├── er/               # canvas, layout, DBML, PNG export
    │       └── agent/            # chat pane, markdown
    │
    ├── app/                      # app-wide prefs and the command keymap (a leaf: features read it)
    └── styles/                   # tokens.css, app.css
```

Subpath imports: `#core`, `#engines`, `#features`, `#lib` (see `package.json` `imports`). Folder barrels are mapped there, so import `#core/ui` rather than the `index` file. A file import still needs the `.js` extension SvelteKit 3 expects.

## Anatomy of a feature

```text
features/workspace/er/
├── index.ts          # PUBLIC surface — the only path other features import
├── types.ts          # types this feature owns
├── api.ts            # the IPC calls it makes
├── layout.ts …       # pure logic
└── ui/               # its Svelte components
```

Inside a feature, import files directly (`../model`, `./ErCanvas.svelte`).
Across features, import the barrel (`#features/workspace/grid`) — never a file
inside one.

## Boundary rules

Direction is **routes → features → engines → core**. `npm run check:boundaries`
enforces it (also part of `npm run check`).

| Zone | May import | Must not |
| ---- | ---------- | -------- |
| `routes/*` | A feature's `index.ts`, its `ui/*Page.svelte`, `#core/*` | Other feature internals; styling beyond the page frame |
| `features/x/*` | Own files, `#core/*`, `#engines`, other features' `index.ts` | Another feature's internals; `routes/*` |
| `app/*` | `#core/*` | Features — it is a leaf they read |
| `engines/*` | `#core/*` | `#features/*` — adapters are data, not screens |
| `core/*` | `core/*` only | Everything domain: engines, MCP, profiles, sessions |

Workspace splits into a kernel and a shell:

- **`workspace/session` is the kernel.** Every feature may import it. It
  imports no UI: no `.svelte` files and nothing under a `ui/` folder.
- **`workspace/shell` is composition only.** Nothing imports it except routes
  (directly, or through the `#features/workspace` aggregate).

The check also fails on any **import cycle** across `src/lib` and `src/routes`.
Type-only imports and dynamic `import()` do not count. A cycle that cannot be
broken yet goes in `CYCLE_ALLOWLIST` in the script, with the reason.

Two more rules that keep the graph readable:

- **Types live next to the code that owns them.** The exception is
  `core/contracts` — those DTOs mirror `mystique-connector-api` and are not ours
  to reshape. Generate them from Rust when we get round to it.
- **No global barrels.** There is no `#lib/utils` or `#lib/types`. Each feature
  exports a small surface from its own `index.ts`.

## Adding a database engine

The frontend twin of `ConnectorRegistry::bootstrap()` — see
[`BACKEND_STRUCTURE.md`](./BACKEND_STRUCTURE.md).

1. Create `src/lib/engines/<engine>/` with `index.ts` and `Fields.svelte`.
2. Implement `EngineAdapter`: connection fields + validation + `plugin_config`,
   language (formatter, CodeMirror dialect, completions), and
   `defaultCapabilities`.
3. Add one `register(<engine>)` line in `engines/registry.ts`.

That is the whole cost. Nothing else in the app learns the engine's name —
**chrome renders from `EngineCapabilities`, never from `if engine === "…"`.** A
live session's capabilities always beat the adapter's defaults; the adapter only
supplies what a session cannot (a form, a dialect, a badge mark).

## Styling

- **Design language (required):** [`DESIGN_LANGUAGE.md`](./DESIGN_LANGUAGE.md)
- **Tailwind v4** + semantic tokens (`src/app.css`, `src/lib/styles/tokens.css`)
- Type: only `text-micro|caption|label|body|body-lg|title|display` — **no** `text-[Npx]`
- Color: only semantic tokens — **no** hex / `bg-gray-*` in components
- Theme via `data-theme` on `<html>` — never `dark:bg-*` for chrome
- Helper: `#core/util`'s `cn` for class composition

## Component rules

1. **Compose, don't copy** — screens use `Button`, `Field`, `Select`, `Card`, etc.
2. **Customise via props** — `variant`, `size`, `tone`, `full`, snippets (`children`, `actions`).
3. **`$bindable` for form values** — parent owns state; primitives stay controlled/bindable.
4. **Spread HTML attrs** — primitives forward `...rest` for `id`, `aria-*`, `data-*`.
5. **Tokens only** — no hard-coded theme hex in components (connection color props are the exception).
6. **Read state, don't drill it** — `workspaceSession`, `workspaceStudio`, `appPrefs`.

## Import style

```ts
import { Button, Field, Input, Select } from '#core/ui'
import type { ConnectionProfile } from '#core/contracts'
import { listEngineCatalog } from '#core/ipc'
import { adapterFor, formatQuery } from '#engines'
import { DataGrid } from '#features/workspace/grid'
import { workspaceSession } from '../session.svelte' // same feature
```

## Where does it go?

| Need | Where |
| ---- | ----- |
| Visual control used in 2+ features | `core/ui` |
| Helper with no domain knowledge | `core/util` |
| A DTO the Rust side also speaks | `core/contracts` |
| Anything that differs per database | `engines/<engine>` |
| Screen, panel, or the logic behind it | `features/<feature>` |
| A shortcut or native-menu command | keymap entry in `app/commands.ts`; the feature registers its handler with `commands.register` (`#core/commands`) |
| Mounting a screen | `routes` |

See also: [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md).
