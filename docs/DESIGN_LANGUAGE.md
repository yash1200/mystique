# Mystique — Design Language (Code)

> **Status:** Canonical implementation design system  
> **Stack:** Tailwind CSS v4 · semantic tokens · Svelte 5  
> **Product UX:** [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md)  
> **Components:** [`COMPONENT_STRUCTURE.md`](./COMPONENT_STRUCTURE.md)

This document is the **code contract**. Every UI class must come from this language.  
**Do not invent** random `text-[13px]`, `text-[#abc]`, or one-off spacing.

---

## 1. Principles

1. **Tokens only** — Color, type, radius, shadow, control height come from the design language.
2. **Semantic over raw** — Prefer `text-body text-fg-secondary` over `text-[13px] text-[#9aa6bd]`.
3. **One theme map** — Light/dark swap CSS variables; components never pair `dark:` / `light:` colors.
4. **Dense by default** — Body UI is 13px (`text-body`); use larger steps sparingly.
5. **Primitives first** — Build screens from `$lib/components/core`; don’t restyle buttons per screen.
6. **No magic numbers** — If a value isn’t in this doc, add it to the language first, then use it.

### Forbidden

| Pattern | Why | Use instead |
| ------- | --- | ----------- |
| `text-[11px]`, `text-[13px]`, `text-[22px]` | Random type | `text-caption`, `text-body`, `text-display` |
| `text-xs` / `text-sm` / `text-base` (default Tailwind names) | Ambiguous vs our scale | Semantic steps below |
| `text-[#…]`, `bg-[#…]`, `border-[#…]` | Breaks theme | `text-fg`, `bg-panel`, … |
| `dark:bg-…` for theme surfaces | Duplicates CSS | Semantic `bg-*` tokens |
| Ad-hoc `h-[34px]` outside controls | Inconsistent density | `h-control`, `h-control-sm`, `h-control-lg` |
| Inline hex for chrome | Not theme-aware | Tokens only |
| New font families in components | Brand drift | `font-sans` / `font-mono` |

**Allowed exception:** dynamic connection identity via `--connection-color` + `conn-tint` / `bg-conn` utilities (never for general chrome).

---

## 2. Theme architecture

```
<html data-theme="dark|light" data-density="compact|comfortable">
        │
        ▼
tokens.css          ← variable values only
        │
        ▼
app.css @theme      ← exposes Tailwind utilities (once)
        │
        ▼
Components          ← semantic classes only
```

| Layer | File | Responsibility |
| ----- | ---- | -------------- |
| Values | `src/lib/styles/tokens.css` | Hex / rgba for light & dark |
| Bridge | `src/app.css` (`@theme inline`) | Map tokens → utilities |
| Primitives | `src/lib/components/core/*` | Encode variants with language classes |
| Features | `src/lib/components/{home,workspace}/*` | Compose primitives + page chrome |

Theme switch = change `data-theme`. **No component rewrites.**  
Density switch = change `data-density`. Appearance prefs live in `src/lib/stores/app-prefs.svelte.ts` and apply at boot (`appPrefs.apply()`).

---

## 3. Color language

### 3.1 Surfaces

| Token class | Role | Example use |
| ----------- | ---- | ----------- |
| `bg-app` | Deepest app canvas | Window background |
| `bg-panel` | Sidebars, cards | Connection rail, card body |
| `bg-elevated` | Cards, tabs, menus, toasts | Must sit above `bg-panel` (light: warm paper white) |
| `bg-input` | Fields, chips | Input, select, badge neutral |
| `bg-hover` | Hover wash | List row hover |
| `bg-active` | Active chrome | (reserved) |
| `bg-selected` | Selection wash | Grid selection (later) |
| `bg-accent` | Primary solid CTA | Primary button |
| `bg-accent-hover` | Primary hover | Primary button hover |
| `bg-accent-muted` | Soft accent fill | Selected engine, icon active |

### 3.2 Text

| Token class | Role |
| ----------- | ---- |
| `text-fg` | Primary labels, titles, body |
| `text-fg-secondary` | Supporting copy, meta |
| `text-fg-muted` | Placeholders, hints, disabled-adjacent |
| `text-fg-inverse` | Text on solid accent / logo mark |
| `text-accent` | Links, text buttons |
| `text-success` | Connected, success |
| `text-warning` | PROD, caution |
| `text-danger` | Errors, delete |
| `text-info` | Informational status (cerulean — **not** jade accent) |

### 3.3 Borders & focus

| Token class | Role |
| ----------- | ---- |
| `border-line` | Default dividers, control borders |
| `border-line-strong` | Emphasized / hover borders |
| `border-focus` | Focus ring color |
| `outline-focus` | `:focus-visible` outline |
| `ring-accent-muted` | Soft focus ring on inputs |

### 3.4 Status (use with text/bg/border)

| Token | Meaning |
| ----- | ------- |
| `success` | Connected, ok |
| `warning` | Production, slow, caution |
| `danger` | Error, destructive |
| `info` | Neutral running / info (`--status-info`, cyan — **not** accent) |

Patterns:

```txt
text-danger border-danger/40 bg-danger/15   ← error chip / alert
text-warning border-warning/45 bg-warning/15 ← PROD pill
```

Opacity steps **allowed:** `/10` `/12` `/15` `/25` `/35` `/40` `/45` `/50` only with status or accent tokens.

### 3.5 Connection colors (identity only)

| Class | Role |
| ----- | ---- |
| `bg-conn-rose` … `bg-conn-slate` | Swatches, static markers |
| `--connection-color` + `conn-tint` | Scoped active connection |

**Never** use connection hues for primary buttons or general chrome.

### 3.6 Syntax highlighting (`syntax.*`)

CodeMirror and any code chrome must use these tokens (light + dark in `tokens.css`):

| Token | Role |
| ----- | ---- |
| `--syntax-keyword` | SQL/MQL keywords |
| `--syntax-string` | Strings |
| `--syntax-number` | Numbers |
| `--syntax-comment` | Comments |
| `--syntax-operator` | Operators (`=`, `>`, `AND`) |
| `--syntax-punct` | Punctuation / brackets |
| `--syntax-name` | Identifiers |
| `--syntax-property` | Properties / fields |
| `--syntax-type` | Types |
| `--syntax-func` | Functions / builtins |
| `--syntax-atom` | Bool / null / atoms |
| `--syntax-invalid` | Errors |

Do not reuse `--accent` or connection hues for syntax. Reconfigure the editor theme when `data-theme` changes (`createMystiqueEditorTheme`).

### 3.7 Color checklist

- [ ] No hex in `.svelte` files (except data models for connection palette in `types/`)
- [ ] No `text-black` / `text-white` / `bg-gray-*` for product chrome
- [ ] Status color always paired with text or icon, not color alone

---

## 4. Typography language

### 4.1 Families

| Class | Use |
| ----- | --- |
| `font-sans` | **Instrument Sans** — UI chrome (default) |
| `font-mono` | **IBM Plex Mono** — hosts, ports, SQL, JSON, grid cells, badges |

### 4.2 Size scale (closed set)

| Class | Size | Line height | Use **only** for |
| ----- | ---- | ----------- | ---------------- |
| `text-micro` | 10px | 1.2 | Badge mono marks, tiny uppercase chips |
| `text-caption` | 11px | 1.35 | Meta, hints, shortcuts, status bar, footers |
| `text-label` | 12px | 1.4 | Field labels, card section titles, secondary UI |
| `text-body` | 13px | 1.4 | **Default UI body**, inputs, buttons, list titles |
| `text-body-lg` | 14px | 1.45 | Emphasized body, brand name in sidebar |
| `text-title` | 18px | 1.3 | Empty states, minor headings |
| `text-display` | 22px | 1.25 | Page / connection titles |

**Default:** `body` uses `text-body`. Do not use Tailwind’s default `text-xs`…`text-xl` for product UI.  
**Density:** `data-density="compact"` is default; `comfortable` loosens `--row-height`, `--sidebar-width`, and `--grid-row-height` (28px → 32px grid rows). Toggle lives in **Settings** (`AppearanceSettings` / `SettingsModal`).

### 4.3 Weight scale (closed set)

| Class | Use |
| ----- | --- |
| `font-normal` (400) | Body, meta |
| `font-medium` (500) | List names, buttons, labels |
| `font-semibold` (600) | Titles, card headers, brand |
| `font-bold` (700) | Logo mark only |

Avoid `font-extrabold` / `font-black`.

### 4.4 Tracking

| Class | Use |
| ----- | --- |
| `tracking-tight` | Display / page titles |
| `tracking-normal` | Default |
| `tracking-wide` | Optional labels |
| `tracking-wider` | Uppercase section headers (`text-label`) |

### 4.5 Typography recipes

| Recipe | Classes |
| ------ | ------- |
| Page title | `text-display font-semibold tracking-tight text-fg` |
| Section header | `text-label font-semibold uppercase tracking-wider text-fg-muted` |
| Field label | `text-label font-medium text-fg-secondary` |
| Body | `text-body text-fg` |
| Meta / hint | `text-caption text-fg-muted` |
| Mono host | `font-mono text-caption text-fg-muted` |
| Link text | `text-body` or `text-caption` + `text-accent` |

---

## 5. Spacing language

Use **Tailwind spacing scale only** (rem-based). Preferred steps for this product:

| Class | px (approx) | Use |
| ----- | ----------- | --- |
| `gap-0.5` / `p-0.5` | 2 | Dense chip gaps |
| `gap-1` / `p-1` | 4 | Inline icon gaps |
| `gap-1.5` / `p-1.5` | 6 | Field label gap |
| `gap-2` / `p-2` | 8 | Default compact gap |
| `gap-3` / `p-3` | 12 | Form grids, toolbars |
| `gap-4` / `p-4` | 16 | Card padding-x, section gaps |
| `gap-5` / `p-5` | 20 | Stack form sections, hero |
| `gap-6` / `p-6` | 24 | Page panel padding-x |
| `gap-8` / `p-8` | 32 | Page panel padding-y |

**Avoid:** `p-[13px]`, `gap-[7px]`, `mt-[18px]`.

Layout widths:

| Token / class | Use |
| ------------- | --- |
| `w-sidebar` | Left connection rail (`var(--sidebar-width)`) |
| `max-w-panel-sm` | Empty states (~420px) |
| `max-w-panel-md` | Forms (~720px) |
| `max-w-panel-lg` | Detail (~880px) |
| `max-w-prose-ui` | Descriptions (~52ch) |
| `h-modal` | Tall dialog fill (`min(36rem, 100vh − 2rem)`) |
| `max-h-modal-sm` · `md` · `lg` | Dialog max heights (32 / 40 / 44rem) |

---

## 6. Radius language

| Class | Value | Use |
| ----- | ----- | --- |
| `rounded-sm` | 3px | Badges, kbd, micro chips |
| `rounded-md` | 5px | Buttons, inputs, list rows |
| `rounded-lg` | 8px | Icon buttons, toasts, brand mark |
| `rounded-xl` | 10px | Cards, heroes |
| `rounded-full` | pill | Pills, status dots |

Do not invent `rounded-[5px]`.

---

## 7. Elevation / shadow

| Class | Use |
| ----- | --- |
| `shadow-none` | Flat chrome |
| `shadow-sm` | Theme toggle active, light elevation |
| `shadow-md` | Toasts, floating panels |

---

## 8. Control sizes

Closed set for interactive controls:

| Class | Height | Use |
| ----- | ------ | --- |
| `h-control-sm` | 28px | Compact buttons, dense toolbars |
| `h-control` | 34px | **Default** buttons, inputs, selects |
| `h-control-lg` | 40px | Rare emphasis |
| `h-topbar` | 44px | App top bar |
| `min-h-row` | `var(--row-height)` | Connection list rows |
| `min-h-grid-row` | `var(--grid-row-height)` | Data grid cells (28 / 32px) |
| `h-statusbar` / `min-h-statusbar` | 34px min | Status footers (may wrap) |

Square controls (icon buttons) use matching edge length:

| Class | Size | Use |
| ----- | ---- | --- |
| `size-control-sm` | 28×28 | Compact icon buttons |
| `size-control` | 34×34 | **Default** icon buttons (aligns with `h-control`) |
| `size-control-lg` | 40×40 | Rare emphasis |

Pair toolbar fields with icon actions at the **same step** (`md` / `h-control` + `size-control`). Never mix `inputSize="sm"` with a default 32px pixel button.

Input padding: `px-2.5` horizontal; textareas `px-2.5 py-2`.

---

## 9. Motion

Tokens live on `:root` in `tokens.css`. Helpers in `$lib/utils/motion`.

| Token | Value | Use |
| ----- | ----- | --- |
| `--ease-out` | `cubic-bezier(0.23, 1, 0.32, 1)` | Enter / exit |
| `--ease-in-out` | `cubic-bezier(0.77, 0, 0.175, 1)` | On-screen movement |
| `--ease-drawer` | `cubic-bezier(0.32, 0.72, 0, 1)` | Sheets (if added) |
| `--duration-press` | `160ms` | `:active` scale |
| `--duration-popover` | `200ms` | Menus, selects |
| `--duration-modal` | `250ms` | Dialogs + backdrops |
| `--duration-toast` | `400ms` | Toasts |

| Class / helper | Use |
| ----- | --- |
| `transition-colors` | Default interactive |
| `duration-100` / `duration-150` | Hovers, theme-adjacent |
| `ease-[var(--ease-out)]` | Transform / opacity transitions |
| `motion-safe:hover:scale-105` | Color swatches only |
| `overlayFade` / `modalPanel` / `popover` / `toastSlide` | Enter + exit (Svelte `transition:`) |
| `motion-reduce:…` | Always respect reduced motion |

Animate `transform` and `opacity` only. Never `scale(0)` — start at `0.95–0.97`. No continuous animation in chrome.

---

## 10. Component recipes (use primitives)

| Need | Primitive | Notes |
| ---- | --------- | ----- |
| Button | `Button` | `variant`: primary \| secondary \| ghost \| danger \| danger-ghost |
| Icon button | `IconButton` | `variant`: default \| accent \| ghost \| quiet. `quiet` is borderless toolbar chrome (explorer, clustered actions) |
| Text field | `Field` + `Input`/`Select`/`Textarea` | Label = `text-label`. `Input inset`: none \| start \| end \| both (icon padding) |
| Card | `Card` | Title recipe = section header |
| Status | `StatusDot` + `Pill` | PROD = `tone="warning"` |
| Engine mark | `EngineBadge` | Uses `text-micro` mono |
| Toast | `Toast` | `tone` only |
| Empty | `EmptyState` | `text-title` + `text-body` + next action; parent centers it |
| Status footer | `StatusBar` | Three zones: left meta · center pager · right actions; may wrap (`min-h-statusbar`) |
| Resize | `Resizable` | Panel wrapper or handle-only sash. `axis` x/y, `edge` start/end, `variant` overlay/split |
| Pager | `Pager` | ‹ n/m › — used inside `StatusBar` center |
| Settings | `SettingsModal` | Gear + modal; panes in `home/settings` |
| Brand | `BrandMark` | Ink-tile dog mark — identity in the home sidebar and workspace rail |
| Shortcut | `Kbd` | Keyboard chips (`⌘T`) |
| Alert | `Alert` | `info` uses `--status-info`, not accent |
| Modal | `Modal` | Overlay + panel. `layout`: inset (confirm/form) · chrome (header/body/footer) · bare (custom shell). `size`: sm · md · lg · xl. `height`: auto · fill · sm · md · lg |

**Variants customize look; `class` only for layout** (`w-full`, `col-span-full`, `mt-3`).

---

## 11. Z-index scale

| Class | Use |
| ----- | --- |
| `z-0` / auto | Normal flow |
| `z-10` | Sticky headers (later) |
| `z-40` | Popovers / menus |
| `z-50` | Toasts / modal dialogs |

Do not use `z-[9999]`.

---

## 12. Layout shells

| Region | Classes (reference) |
| ------ | ------------------- |
| App root | `flex h-screen w-screen overflow-hidden bg-app` |
| Sidebar | `flex h-full w-sidebar flex-col border-r border-line bg-panel` |
| Top bar | `flex h-topbar shrink-0 items-center … border-b border-line bg-panel` |
| Main | `flex min-w-0 flex-1 flex-col` |
| Content scroll | `flex-1 overflow-auto bg-app` (+ optional subtle accent wash utility) |
| Centered panel | `ContentPanel` → `max-w-panel-*` |

---

## 13. Atmosphere

**Grain:** `body::after` uses `--grain-opacity` (dark 0.045 / light 0.055). Do not add extra noise in features.

**Wash:** use the utility (not raw gradient strings):

```html
<div class="bg-app-wash flex-1 overflow-auto">…</div>
```

Washes use `--connection-color` when scoped, otherwise `--accent`. Defined once in `app.css`.

---

## 14. Accessibility (design-language aligned)

- Focus: never remove outline without `focus-visible:outline-focus`
- Contrast: `text-fg-muted` only for non-essential meta
- Hit targets: controls ≥ `h-control-sm` (28px); prefer `h-control` / `size-control`
- Toolbar row: Input + IconButton share one size step (`md` = 34px)
- Status: always text/icon + color
- Selection: app chrome is `user-select: none`. Allow text in inputs, the query editor, and data cells (`[data-grid-cell]`, `[data-result-cell]`).

---

## 15. PR / review checklist

Before merging UI:

- [ ] No `text-[…px]` or arbitrary font sizes  
- [ ] No hex colors in components  
- [ ] Type uses only: micro · caption · label · body · body-lg · title · display  
- [ ] Colors only from §3  
- [ ] Spacing from §5 scale  
- [ ] Controls use `h-control*`  
- [ ] Light **and** dark checked  
- [ ] New patterns added to this doc if needed  

---

## 16. Extending the language

1. Propose token in this doc (name, value, usage).  
2. Add value to `tokens.css` if theme-dependent.  
3. Map in `app.css` `@theme inline`.  
4. Use the new class in primitives first.  
5. Never skip straight to arbitrary utilities in a feature.

---

## 17. Quick reference card

```txt
Type:     text-micro | text-caption | text-label | text-body | text-body-lg | text-title | text-display
Weight:   font-normal | font-medium | font-semibold | font-bold
Color fg: text-fg | text-fg-secondary | text-fg-muted | text-fg-inverse | text-accent | text-success|warning|danger|info
Surface:  bg-app | bg-panel | bg-elevated | bg-input | bg-hover | bg-accent | bg-accent-muted
Border:   border-line | border-line-strong | border-focus
Control:  h-control-sm | h-control | h-control-lg
Radius:   rounded-sm | rounded-md | rounded-lg | rounded-xl | rounded-full
Shadow:   shadow-sm | shadow-md
Font:     font-sans (Instrument Sans) | font-mono (IBM Plex Mono)
Accent:   jade #2fbf96 / #0e7a60 — never connection hue
```

---

*If it’s not on this card, don’t ship it — extend the language first.*
