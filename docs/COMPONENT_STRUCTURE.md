# Frontend component structure

> Customisable, reusable UI for Mystique (Svelte 5).

## Layers

```
src/lib/
├── styles/tokens.css          # Theme tokens only (no components)
├── types/                     # Shared TS models
├── data/                      # Mock / fixtures (UI-only for now)
└── components/
    ├── core/                  # Design-system primitives (Button, Modal, Input, …)
    ├── home/                  # Home / connections + app chrome (settings, brand)
    └── workspace/             # Connected studio
        ├── er_diagram/        # ER / model canvas, DBML, layout
        ├── explorer/
        ├── grid/
        ├── query/
        └── shell/
```

| Layer | May know about | Must not |
| ----- | -------------- | -------- |
| `core/*` | Variants, tokens, a11y, layout primitives | Product domain (engines, MCP, shortcuts) |
| `home/*` | Connection profiles, engines, app settings | Re-implement button/input styles |
| `workspace/*` | Live sessions, tabs, catalog | Re-implement button/input styles |
| `routes/*` | Wire stores + feature components | Deep visual styling |

## Styling

- **Design language (required):** [`DESIGN_LANGUAGE.md`](./DESIGN_LANGUAGE.md)
- **Tailwind v4** + semantic tokens (`src/app.css`, `src/lib/styles/tokens.css`)
- Type: only `text-micro|caption|label|body|body-lg|title|display` — **no** `text-[Npx]`
- Color: only semantic tokens — **no** hex / `bg-gray-*` in components
- Theme via `data-theme` on `<html>` — never `dark:bg-*` for chrome
- Helper: `$lib/utils/cn` for class composition

## Rules

1. **Compose, don’t copy** — screens use `Button`, `Field`, `Select`, `Card`, etc.
2. **Customise via props** — `variant`, `size`, `tone`, `full`, snippets (`children`, `actions`).
3. **`$bindable` for form values** — parent owns state; primitives stay controlled/bindable.
4. **Spread HTML attrs** — primitives forward `...rest` for `id`, `aria-*`, `data-*`.
5. **Tokens only** — no hard-coded theme hex in components (connection color props are the exception).
6. **New feature folder** when a domain grows (`workspace/`, `query/`) — still compose `core/*`.

## Import style

```ts
import { Button, Field, Input, Select } from "$lib/components/core";
import { SettingsModal, BrandMark } from "$lib/components/home";
import ConnectionSidebar from "$lib/components/home/ConnectionSidebar.svelte";
```

## Adding UI

| Need | Where |
| ---- | ----- |
| New visual control used in 2+ places | `components/core` |
| Home / connections / app chrome (settings, shortcuts, brand) | `components/home` (`home/settings` for the settings modal) |
| Workspace studio widget | `components/workspace` |
| Page layout / state wiring | `routes` |

See also: [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md).
