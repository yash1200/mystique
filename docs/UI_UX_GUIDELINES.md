# MystiqueDB — UI / UX Guidelines

> **Status:** Canonical product UX (follow before implementing UI)  
> **Code design language (Tailwind tokens):** [`DESIGN_LANGUAGE.md`](./DESIGN_LANGUAGE.md)  
> **Companion:** [`ARCHITECTURE_AND_FEATURES.md`](./ARCHITECTURE_AND_FEATURES.md)  
> **Date:** 2026-08-09

These rules define how MystiqueDB looks and behaves. Every screen, component, and interaction should pass this checklist. When in doubt: **clarity under density, calm chrome, loud data.**

---

## 1. Design philosophy

### 1.1 Product personality

| Attribute | Direction |
| --------- | --------- |
| Tone | Laboratory notebook: warm ink, precise, a little analog. Not a marketing site. |
| Metaphor | **Mystique** — a lens on data. Chrome is the instrument; the engine is the specimen. |
| Density | Information-dense by default, with optional Comfortable density. |
| Motion | Subtle and functional. Grain is static. No decorative parallax. |
| Differentiation | Warm dual theme, jade **signal** accent, connection **color identity**, dual-paradigm views. Not Inter, not purple gradients. |

### 1.2 Core principles

1. **Data first, chrome second** — UI chrome stays quiet; grids, JSON, and SQL get contrast and focus.
2. **Orientation always** — User always knows *which connection*, *which object*, *which tab* they are in (name + color + engine icon).
3. **Predictable structure** — Same shell across engines; only the content region morphs (Mystique principle).
4. **Keyboard-native** — Every primary action has a shortcut; mouse is optional for power users.
5. **Safe by default** — Destructive actions require confirmation; write previews where possible; production connections get stronger visual caution.
6. **Accessible dual theme** — Light and dark are first-class, not an afterthought skin.
7. **Connection color is identity** — Each connection has a user-chosen accent used consistently in tabs, tree, and status.

### 1.3 Anti-patterns (do not)

- Generic “AI SaaS” look: Inter-everywhere, purple-on-white gradients, glassmorphism for its own sake.
- Rainbow UI unrelated to connection colors (reserve hue for identity and status).
- Modal dialogs for routine browsing (prefer panels, tabs, inline).
- Fake “capabilities preview” chips that are not live plugin data.
- Hidden critical status (connection dead, query running, uncommitted edits).
- Different layout systems per engine that retrain the user from scratch.

---

## 2. Theme system

### 2.1 Theme modes

| Mode | Behavior |
| ---- | -------- |
| **Light** | Explicit light palette. |
| **Dark** | Explicit dark palette (default for first launch is fine for a DB tool). |
| **System** | Follow OS `prefers-color-scheme`; update live when OS changes. |

**Storage:** Persist user choice in app settings (`theme: "light" | "dark" | "system"`).

**Application:** Set `data-theme="light" | "dark"` on `<html>` (resolve `system` before applying). All colors come from CSS variables—never hard-code hex in components except in the token file.

### 2.2 Theme structure

Themes are **semantic tokens**, not raw brand colors. Components only use tokens like `--bg-app`, `--text-primary`, `--border-subtle`.

```
tokens
├── primitive/     # raw scales (neutral, accent base, status)
├── semantic/      # bg, text, border, interactive, status
└── component/     # tab, grid, editor, sidebar (optional aliases)
```

### 2.3 Semantic color roles

| Token role | Purpose |
| ---------- | ------- |
| `bg.app` | Deepest app background |
| `bg.panel` | Sidebar, secondary surfaces |
| `bg.elevated` | Tabs, popovers, dropdowns |
| `bg.input` | Inputs, editors gutter-adjacent |
| `bg.hover` | Row / tree / tab hover |
| `bg.active` | Selected row / active tree node |
| `bg.selected` | Multi-select / focused grid row |
| `border.subtle` | Dividers, grid lines |
| `border.strong` | Focused panels, modals |
| `border.focus` | Keyboard focus ring (high contrast) |
| `text.primary` | Titles, primary labels |
| `text.secondary` | Meta, timestamps, types |
| `text.muted` | Placeholders, disabled |
| `text.inverse` | Text on solid accent buttons |
| `accent.*` | App-level accent (not connection color) for primary buttons, links |
| `status.success / warning / danger / info` | Query ok, slow, error, running |
| `syntax.*` | Code highlighting (SQL/JSON) — theme-aware |

### 2.4 Recommended primitive palettes

Use a **warm ink / warm paper** base (olive-black and cream, not blue-gray) so jade signal and connection hues stay distinct.

#### Dark (default reference)

| Role | Value | Notes |
| ---- | ----- | ----- |
| `bg.app` | `#131210` | Warm near-black |
| `bg.panel` | `#1b1a17` | Sidebar |
| `bg.elevated` | `#24221e` | Cards, menus |
| `bg.input` | `#151410` | Inputs / editor |
| `border.subtle` | `#34312b` | |
| `text.primary` | `#f0ebe3` | Ivory |
| `text.secondary` | `#b4ab9d` | |
| `text.muted` | `#7d766a` | |
| `accent` | `#2fbf96` | Jade signal (not connection teal) |
| `status.danger` | `#e35d5d` | |
| `status.success` | `#5ecf8a` | |
| `status.warning` | `#e0b04a` | |
| `status.info` | `#4aa8d8` | Cerulean — distinct from jade |

#### Light

| Role | Value | Notes |
| ---- | ----- | ----- |
| `bg.app` | `#efeae1` | Warm paper canvas |
| `bg.panel` | `#f7f3eb` | Sidebar |
| `bg.elevated` | `#fffdf8` | Cards, menus |
| `bg.input` | `#ebe6dc` | |
| `border.subtle` | `#d8d0c2` | |
| `text.primary` | `#1c1914` | Ink |
| `text.secondary` | `#534c42` | |
| `text.muted` | `#6f685c` | AA-safe on paper |
| `accent` | `#0e7a60` | Deeper jade for AA on cream |
| `status.danger` | `#c4333a` | |
| `status.success` | `#1e8a4c` | |
| `status.warning` | `#9a7208` | |
| `status.info` | `#176a96` | |

Exact hex may be refined in implementation; **token names and contrast rules are binding**.

### 2.5 Contrast requirements

- Body text on background: **WCAG AA** minimum (4.5:1 for normal text).
- Muted text: still readable for secondary labels; if it fails AA, use only for decorative chrome.
- Focus rings: 2px+ and contrast ≥ 3:1 against adjacent background.
- Grid selection + connection tint must not reduce cell text below AA—use **tinted backgrounds at low alpha**, not recolored text.
- Syntax highlighting must remain legible in both themes (separate `syntax.*` sets).

### 2.6 Surfaces & elevation

| Level | Dark | Light |
| ----- | ---- | ----- |
| 0 App | Flat `bg.app` | Flat `bg.app` |
| 1 Panel | Border only | Border + very soft shadow |
| 2 Popover / modal | Border + shadow | Border + medium shadow |
| 3 Toast | Elevated + status accent bar | Same |

Avoid heavy skeuomorphism. Prefer **1px borders** and restrained shadows.

### 2.7 Theme toggle UX

- Location: **Settings** (`AppSettingsPopover`) — home sidebar footer **and** workspace sessions-rail footer. Not a home titlebar.
- Settings also expose **density** (compact / comfortable), **UI scale**, and **Open row details on click** (default on; off = Show details in the row menu). Home always opens on **New connection**; pick a saved connection from the sidebar.
- Persistence: `src/lib/utils/appPrefs.ts` (localStorage). Applied at boot in `+layout.svelte` via `applyStoredAppearance()`.
- Icons: sun / moon / monitor (system).
- Switching themes must **not** remount the whole workspace (preserve tabs, scroll, selection). CodeMirror reconfigures via `data-theme` observer.
- Connection colors remain the same hue in both themes; only **alpha / surface mix** changes (see §3).
- `accent` (primary CTA) and `status.info` (running / info) are **different hues**.

---

## 3. Connection colors (identity system)

Users run many databases at once. Color is how they tell **prod Postgres** from **local Mongo** at a glance.

### 3.1 Rules

1. Every connection has a **`colorId`** (or custom hex) chosen at create/edit time.
2. That color appears in **all** surfaces tied to that connection:
   - Connection list row indicator
   - Connection tab leading stripe / dot
   - Workspace tab secondary accent (thin bar or left edge)
   - Active sidebar header strip when that connection is focused
   - Optional status-bar connection chip
3. Color is **user-controlled**, not forced by engine type (engine has its own icon).
4. Default assignment: cycle the palette when creating connections so two new ones are not the same.
5. User may change color later; update all open chrome immediately.
6. **Production** flag (optional connection property) adds a persistent warning treatment (amber badge / “PROD” pill)—independent of color.

### 3.2 Preset palette

Provide a curated set of **12 distinct hues** that remain distinguishable in both themes and from status colors (red/green reserved somewhat for status—connection red is allowed but use a different shade).

| ID | Name | Hue (approx.) | Hex reference | Use suggestion |
| -- | ---- | ------------- | ------------- | -------------- |
| `rose` | Rose | 350° | `#e86a7a` | |
| `orange` | Orange | 25° | `#e8956a` | |
| `amber` | Amber | 40° | `#e6b84d` | Avoid for prod badge conflict if possible |
| `lime` | Lime | 85° | `#a8d46a` | |
| `green` | Green | 145° | `#5ecf8e` | |
| `teal` | Teal | 175° | `#4ecdc4` | |
| `cyan` | Cyan | 195° | `#4eb8e8` | |
| `blue` | Blue | 215° | `#5a8ee8` | Distinct from jade app accent |
| `indigo` | Indigo | 245° | `#7b7ef0` | |
| `violet` | Violet | 270° | `#9b6de8` | |
| `magenta` | Magenta | 300° | `#d46ad8` | |
| `slate` | Slate | 220° | `#8b95a8` | Neutral / unlabeled |

**Custom color:** Advanced option — any hex; validate contrast for indicator-on-panel (dot may sit on dark/light panel).

### 3.3 How color is applied (recipes)

Never flood the entire UI with the connection hue. Use **accents**:

| Element | Treatment |
| ------- | --------- |
| Connection tab (inactive) | 3–4px left stripe or 8px circle in connection color |
| Connection tab (active) | Stripe + soft background tint `color-mix(in srgb, var(--conn) 12%, var(--bg-elevated))` |
| Workspace tab | 2px top micro-stripe in parent connection color (`bg-conn`); stronger when active |
| Sessions rail | Color-capped monogram tile (name initial); **PROD** warning pip; right-click for refresh / disconnect |
| Workspace shell | `--connection-color` scoped on the workspace root; `data-env="production"` when applicable |
| Sidebar tree root | Small color chip beside connection name |
| Query running indicator | Spinner uses connection color |
| Window / title subtitle | Optional: colored bullet before connection name |

**CSS pattern:**

```css
:root {
  /* set per active connection context on a subtree */
  --connection-color: #3ec9c0;
  --connection-tint: color-mix(in srgb, var(--connection-color) 14%, transparent);
  --connection-tint-strong: color-mix(in srgb, var(--connection-color) 22%, transparent);
}

[data-theme="light"] {
  /* slightly stronger tint on light surfaces if needed */
  --connection-tint: color-mix(in srgb, var(--connection-color) 10%, transparent);
}
```

Scope `--connection-color` on the **workspace subtree** for the active connection so nested tabs inherit identity without prop-drilling styles.

### 3.4 Color picker UI

**Placement:** Connection form (create + edit) — required field with default.

**Control design:**

```
Label: Color
[●][●][●][●][●][●]  ← preset swatches, selected = ring + check
[●][●][●][●][●][●]
[ Custom… ]          ← opens native or compact hex picker
```

- Selected swatch: 2px focus ring using `border.focus` + white/black inner tick.
- Keyboard: arrow keys move between swatches; Enter selects.
- Show live preview: fake mini-tab with name + stripe while editing.
- Accessibility: each swatch `aria-label` is the color name, not only “color 3”.

### 3.5 Disambiguation when colors collide

If two connections share a color:

- Still distinguish by **name + engine icon**.
- Optional: warn “Another connection uses this color” (non-blocking).
- Do not auto-recolor without consent.

### 3.6 Engine icons vs connection color

| Signal | Encodes |
| ------ | ------- |
| Engine icon (Postgres elephant-style mark, Mongo leaf, etc.) | **Technology** |
| Connection color | **Environment / user meaning** (prod, staging, personal…) |
| Name | Human label |

Never rely on color alone for engine type.

---

## 4. Typography

### 4.1 Font stack

Desktop tool → prioritize **legibility and OS harmony**, with a slightly distinctive UI face.

| Role | Font | Fallback |
| ---- | ---- | -------- |
| UI (labels, tabs, tree) | **Instrument Sans** | `ui-sans-serif, system-ui, sans-serif` |
| Code / SQL / JSON / grid cells | **IBM Plex Mono** | `ui-monospace, SFMono-Regular, Menlo, monospace` |
| Numerics in grids | Same mono; consider `font-variant-numeric: tabular-nums` | |

Bundle woff2 subsets or use system mono if package size is critical; prefer one family family-pair (Plex Sans + Plex Mono) for cohesion.

**Avoid:** Inter as the “default AI UI” signature; comic / display / script fonts.

### 4.2 Scale

| Token | Size | Weight | Use |
| ----- | ---- | ------ | --- |
| `text.xs` | 11px | 400/500 | Badges, grid headers (dense), shortcuts |
| `text.sm` | 12px | 400/500 | Tree nodes, secondary labels, tabs |
| `text.md` | 13px | 400 | Body UI default |
| `text.lg` | 14–15px | 500/600 | Section titles, modal titles |
| `text.xl` | 18px | 600 | Rare (empty states, onboarding) |
| `code.sm` | 12px | 400 | Editor default, JSON |
| `code.md` | 13px | 400 | Comfortable editor |

Line-height: UI ~1.35–1.45; code ~1.5.

### 4.3 Rules

- Tab labels and tree labels: **single line + ellipsis**; full name in tooltip.
- Do not bold entire sidebars—weight creates hierarchy only for active/section headers.
- SQL keywords styling is editor-theme responsibility, not global bold.

---

## 5. Layout & chrome

### 5.1 App shell (fixed regions)

```
┌────────────────────────────────────────────────────────────────┐
│ Title bar / traffic lights (native) + optional drag region     │
├──────┬─────────────────────────────────────────────────────────┤
│      │ Connection tabs                                          │
│ Side ├─────────────────────────────────────────────────────────┤
│ bar  │ Workspace tabs (tables, queries, column views…)          │
│      ├──────────────────┬──────────────────────────────────────┤
│ Conn │ Object tree      │ Main: editor + results / grid / JSON  │
│ list │ (per connection) │                                       │
│ +    │                  │                                       │
│ objs │                  │                                       │
│      │                  ├──────────────────────────────────────┤
│      │                  │ Status bar                            │
└──────┴──────────────────┴──────────────────────────────────────┘
```

| Region | Width / height | Behavior |
| ------ | -------------- | -------- |
| Sidebar (connection list) | 200–240px default; collapsible to icons | Resizable; remember width |
| Object tree | 220–280px default | Resizable; independent scroll |
| Connection tab bar | ~36px | Horizontal scroll if overflow; no wrap |
| Workspace tab bar | ~32px | Same |
| Status bar | 34px min (`min-h-statusbar`) | Row range · pager · actions (`StatusBar`) |
| Main pane | flex | Split editor/results vertically (resizable) |

Minimum window: **1024×640**. Ideal: **1440×900+**.

### 5.2 Split panes

- Vertical split: editor (top) / results (bottom); default ~40/60.
- Collapse editor when browsing table data only; collapse results when writing long SQL (user toggle).
- Double-click sash resets to default ratio.
- Persist split ratios per workspace tab type when practical.

### 5.3 Tabs

**Connection tabs (primary):**

- Show: color mark + engine icon + name.
- Active: elevated bg + stronger color stripe.
- Middle-click or `⌘W` closes (confirm if running queries / dirty).
- Drag reorder.
- Sessions rail `+` opens the **Open connection** modal (saved profiles not already in this workspace).
- Mystique mark is identity only — it does **not** return home. Last remaining disconnect still does.


**Workspace tabs (secondary):**

- Show: object icon + name; dirty state `•`.
- Inherit parent connection color as micro-accent.
- Overflow: dropdown “open tabs” list with search.

### 5.4 Empty states

Use the `EmptyState` primitive (title + optional description + actions). Parent centers it.

- Loading home: “Loading connections…”
- Home always opens on **New connection**. Sidebar lists saved connections; none are preselected.
- Connection selected, no tab: connection identity card + `EmptyState` (“No tab open”) with `⌘/Ctrl+T`.
- Catalog/table errors: show the error and what to try next (retry / pick another table).
- Empty query results: calm message, not error styling.

---

## 6. Density

| Mode | Grid row (`--grid-row-height`) | List row (`--row-height`) | Sidebar |
| ---- | ------------------------------ | ------------------------- | ------- |
| **Compact** (default) | 28px (`1.75rem`) | 40px (`2.5rem`) | 17.5rem |
| **Comfortable** | 32px (`2rem`) | 46px (`2.875rem`) | 19rem |

User setting in **Settings**; apply via `data-density="compact" | "comfortable"` on `<html>`. Query result virtualization reads the same height via `gridRowHeightPx()`.

---

## 7. Component guidelines

### 7.1 Buttons

| Variant | Use |
| ------- | --- |
| Primary | One main action per view (Run query, Connect, Import) |
| Secondary | Cancel, alternative |
| Ghost | Toolbar icons, tertiary |
| Danger | Drop, delete connection, destructive apply |

- Height: 28px compact / 32px comfortable.
- Radius: **6px** (controls), **8px** (cards/modals).
- Primary uses **app accent**, not connection color (connection color is identity, not CTA).

### 7.2 Inputs

- Height aligned with buttons.
- Focus: `border.focus` + 2px ring (offset).
- Invalid: `status.danger` border + helper text below.
- Labels above fields (not only placeholder).

### 7.3 Data grid

- Sticky header; zebra optional (very subtle, theme-aware).
- Selection: row bg `bg.selected`; cell edit mode clear outline.
- Nulls: muted italic `NULL` — never an empty-looking hole.
- **Page size is fixed at 100** for table data and query results. No page-size dropdown.
- Footer uses `StatusBar` + `Pager`: left = row range, center = pager, right = actions.
- Table pager fetches the next page from the engine plugin (`OFFSET`); query results page client-side after fetch (max 50k).
- Dirty cells: warning wash + save/reset in the footer.
- Focused cell: accent inset ring; editing: input fills the cell.

### 7.4 Object tree

- Indent 12–14px per level.
- Icons for schema / table / view / collection / index.
- Search filter sticky at top of tree.
- Database picker lists **every connectable database** on that server (Postgres / MySQL). Switching reconnects the SQL session to the chosen database. SQLite stays a single file (`main`).
- Right-click context: Open, Open in Column View, Copy name, Refresh…
- Active object matches open tab when possible (sync highlight).

### 7.5 Editors (SQL / JSON / Mongo)

- Full-area code editor (CodeMirror 6); highlight colors from `--syntax-*` tokens only.
- Gutter line numbers; active line subtle highlight.
- Run: primary toolbar button + `⌘Enter` / `Ctrl+Enter`.
- Running: disable double-submit; show stop (cancel).
- Errors: panel below editor or inline marker—not only toast.

### 7.6 Column View

- Card grid or vertical list of column/field profiles.
- Each card: name, type chip, null %, distinct, mini histogram / top values.
- Cards use neutral surfaces; type chips use muted semantic colors (not connection color).
- Click card → focus/filter related grid column when linked.

### 7.7 Dialogs & sheets

- Prefer **right sheet** for JSON document edit / import wizard steps.
- Use modal only for: confirm destroy, first-run, blocking errors.
- Escape closes non-destructive dialogs; confirm dirty close.

### 7.8 Toasts & status

- Toast: bottom-right, auto-dismiss 4–6s; errors sticky until dismissed.
- Prefer status bar for “Connected · 14ms · 120 rows”.
- Never rely on toast alone for failed queries—show in results pane.

### 7.9 Icons

- Stroke icons (Lucide or similar), 16px in chrome, 14px in dense lists.
- Consistent set—do not mix filled/outline randomly.
- Engine icons may be custom simple marks (single-color, currentColor + optional connection chip).

---

## 8. Interaction & UX patterns

### 8.1 Keyboard (minimum)

| Shortcut | Action |
| -------- | ------ |
| `⌘/Ctrl + Enter` | Run query / statement |
| `⌘/Ctrl + Shift + Enter` | Run selection |
| `⌘/Ctrl + W` | Close tab |
| `⌘/Ctrl + T` | New query tab |
| `⌘/Ctrl + L` | Focus object filter / tree search |
| `⌘/Ctrl + 1…9` | Switch workspace tabs |
| `⌘/Ctrl + Shift + [` / `]` | Prev/next connection tab |
| `⌘/Ctrl + B` | Toggle sidebar |
| `Enter` | Edit selected grid cell |
| `Shift + Enter` | Open selected cell in the value editor (Text / JSON / HTML) |
| `Escape` | Clear selection / close popover / cancel cell edit |
| `/` or `⌘/Ctrl + F` | Find in grid / editor (context-aware) |

Show shortcuts in tooltips and the command palette (future).

### 8.2 Command palette (planned)

`⌘/Ctrl + K` — jump connections, tables, commands. Results show connection color dots.

### 8.3 Loading & progress

- Tree load: skeleton lines (not full-page spinner).
- Query: progress in tab + cancel.
- Export/import large files: determinate progress modal/sheet with bytes/rows.

### 8.4 Destructive flows

1. Explicit label (“Delete connection `prod-mongo`”).
2. Confirm button danger-styled.
3. For data loss (truncate, drop): type object name to confirm (optional advanced).

### 8.5 Write safety

- Inline cell edits: batch + “Review changes” (SQL/ops preview) before apply.
- Mongo JSON save: validate parse; show diff summary when possible.
- Read-only connection mode: hide edit affordances; badge “Read-only”.

### 8.6 Mystique adaptation (engine morph)

Same chrome; content swaps:

| | Relational | Document |
| - | ---------- | -------- |
| Tree | schemas → tables | databases → collections |
| Default open | Data grid | Document list / JSON |
| Editor | SQL | find / aggregation / JSON |
| Column View | SQL columns | Inferred field paths |

Transition: no full-app animation—instant content swap; keep sidebar structure.

---

## 9. Motion

| Interaction | Motion |
| ----------- | ------ |
| Theme switch | Cross-fade surfaces ≤ 150ms or instant if `prefers-reduced-motion` |
| Tab switch | Instant content; optional 100ms fade |
| Sidebar collapse | Width transition 160ms ease-out |
| Dropdown | 80–120ms opacity + 4px translate |
| Toasts | Slide + fade |

**Respect `prefers-reduced-motion: reduce`:** disable non-essential transitions.

No continuous ambient animation in the shell.

---

## 10. Accessibility

1. Full keyboard navigation for tree, tabs, grid (as far as grid lib allows).
2. Focus visible always (never `outline: none` without replacement).
3. `aria-selected` on tabs; `aria-current` where appropriate.
4. Live region for query errors (`aria-live="polite"`).
5. Connection color not the only status signal—pair with icon/text.
6. Hit targets ≥ 24px in compact mode for primary controls (dense trees may be 22px with careful spacing).
7. High-contrast: ensure selection and borders remain visible on both themes.

---

## 11. Content & copy

| Do | Don't |
| -- | ----- |
| “Run query” | “Execute operationalization” |
| “Connection failed: timeout” + hint | Generic “Error” |
| “NULL” for SQL nulls | Blank cells without meaning |
| “Unsaved changes” | Silent tab close |
| Engine-specific terms when connected (collection vs table) | Wrong noun for engine |

Language: American English default; keep strings centralizable for i18n later.

---

## 12. Implementation contract (Svelte + CSS)

### 12.1 Root attributes

```html
<html
  data-theme="dark"
  data-density="compact"
  data-accent="default"
>
```

### 12.2 Token file

Single source: `src/lib/styles/tokens.css` (or `.scss`) defining:

- Primitive scales  
- Semantic mappings per `[data-theme="light"]` / `[data-theme="dark"]`  
- Density spacing vars under `[data-density]`  

### 12.3 Connection context

```svelte
<div
  class="workspace"
  style="--connection-color: {connection.colorHex}"
  data-connection-id={connection.id}
>
  ...
</div>
```

### 12.4 Appearance prefs

```ts
// src/lib/utils/appPrefs.ts
type ThemePref = "light" | "dark" | "system";
type DensityPref = "compact" | "comfortable";

applyStoredAppearance(); // +layout onMount
// resolved theme is always light | dark on <html data-theme>
// matchMedia when preference === "system"
```

### 12.5 Component rules

- No raw hex in `.svelte` files except connection palette data.
- Prefer CSS variables for hover/active.
- Shared primitives under `src/lib/components/ui/` (`Button`, `Input`, `StatusBar`, `Pager`, `AppSettingsPopover`, …).
- Feature chrome under `src/lib/components/home/` and `src/lib/components/workspace/`.

### 12.6 Checklist before merging UI

- [ ] Works in light and dark  
- [ ] Connection color visible on tab + tree  
- [ ] Focus states visible  
- [ ] Compact density usable  
- [ ] No horizontal overflow at 1024px width  
- [ ] Reduced motion respected  
- [ ] Destructive actions confirmed  
- [ ] Strings use engine-correct terminology  

---

## 13. Visual reference summary

| Decision | Choice |
| -------- | ------ |
| Default theme | Dark (user may pick Light / System) |
| Themes | Warm ink (dark) / warm paper (light) / System |
| App accent | Jade (`#2fbf96` dark / `#0e7a60` light) |
| Info status | Cerulean (`#4aa8d8` / `#176a96`) — never jade |
| Connection identity | 12 presets; stripe/dot/tint; `--connection-color` on workspace |
| Radius | 3 / 5 / 8 / 10px |
| UI font | Instrument Sans |
| Mono font | IBM Plex Mono |
| Atmosphere | Subtle grain overlay (`--grain-opacity`); connection-tinted wash |
| Default density | Compact |
| Grid row | 28px compact / 32px comfortable |
| Settings | `AppSettingsPopover` on home + workspace |
| Icon style | 16px stroke; brand = jade lens |
| Primary metaphor | Laboratory instrument, adaptive content |

---

## 14. Settled decisions

1. **Fonts:** Instrument Sans (UI) + IBM Plex Mono (data) via Google Fonts.  
2. **Title bar:** native Tauri decorations.  
3. **Connection color:** stripe / swatch / soft tint only — never flood the tab bar.  
4. **Command palette:** deferred (Phase 2).  
5. **High-contrast theme:** deferred; light/dark + density cover the current need.  
6. **Page size:** fixed 100; no dropdown.  
7. **Appearance store:** `appPrefs.ts` (localStorage), not a Svelte store.

---

## 15. Related docs

- Architecture & features: [`ARCHITECTURE_AND_FEATURES.md`](./ARCHITECTURE_AND_FEATURES.md)  
- When implementing screens, treat this document as the **source of truth for look and interaction**; update it when deliberate design changes ship.

---

*MystiqueDB UI: quiet shell, sharp data, and a color for every connection.*
