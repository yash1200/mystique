# Frontend component structure

> Customisable, reusable UI for MystiqueDB (Svelte 5).

## Layers

```
src/lib/
├── styles/tokens.css          # Theme tokens only (no components)
├── types/                     # Shared TS models
├── data/                      # Mock / fixtures (UI-only for now)
└── components/
    ├── ui/                    # Design system primitives (generic)
    │   ├── index.ts           # Barrel export
    │   ├── README.md
    │   ├── Button.svelte
    │   ├── Input.svelte
    │   ├── Select.svelte
    │   └── …
    └── home/                  # Home / connections feature
        ├── EnginePicker.svelte      # Step 1: choose Postgres / Mongo / SQLite
        ├── ConnectionSidebar.svelte
        ├── ConnectionDetail.svelte
        ├── NewConnectionPanel.svelte  # Step 2: engine-locked form
        └── …
```

| Layer | May know about | Must not |
| ----- | -------------- | -------- |
| `ui/*` | Variants, tokens, a11y, layout primitives | Postgres, connection save flow, routes |
| `home/*` (feature) | Connection profiles, engines | Re-implement button/input styles |
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
6. **New feature folder** when a domain grows (`workspace/`, `query/`) — still compose `ui/*`.

## Import style

```ts
import { Button, Field, Input, Select } from "$lib/components/ui";
import ConnectionSidebar from "$lib/components/home/ConnectionSidebar.svelte";
```

## Adding UI

| Need | Where |
| ---- | ----- |
| New visual control used in 2+ places | `components/ui` |
| Connection-specific widget | `components/home` |
| Page layout / state wiring | `routes` |

See also: [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md).
