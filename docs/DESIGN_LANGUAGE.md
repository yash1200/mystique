# Mystique — Design Language (Code)

> **Status:** Canonical implementation design system — **v1, "Quiet surfaces, loud data"**  
> **Stack:** Tailwind CSS v4 · semantic tokens · Svelte 5  
> **Product UX:** [`UI_UX_GUIDELINES.md`](./UI_UX_GUIDELINES.md)  
> **Components:** [`COMPONENT_STRUCTURE.md`](./COMPONENT_STRUCTURE.md)

This document is the **code contract**. Every UI class must come from this language.  
**Do not invent** random `text-[13px]`, `text-[#abc]`, or one-off spacing.

Chrome stays neutral and recessive so connection colours, environment badges and
query results carry the signal. Every theme is a neutral ramp crossed with one
accent ramp, mapped onto the same semantic tokens.

> Every screen is on v1. The pre-v1 names (`bg-panel`, `bg-elevated`,
> `bg-input`, `accent-muted`, `success`, `--bg-app`, `--text-primary`…) are
> gone; use only the names in §3.

---

## 1. Principles

1. **Tokens only** — Color, type, radius, shadow, control height come from the design language.
2. **Semantic over raw** — Prefer `text-body text-fg-secondary` over `text-[13px] text-[#9aa6bd]`.
3. **One theme map** — Light/dark swap CSS variables; components never pair `dark:` / `light:` colors.
4. **Dense by default** — Body UI is 13px (`text-body`); use larger steps sparingly.
5. **Primitives first** — Build screens from `#core/ui`; don’t restyle buttons per screen.
6. **Amber means production.** Never use `warning` for decoration.
7. **Data is mono.** Hosts, ports, keys, SQL and times are `font-mono` (tabular numbers).

### Forbidden

| Pattern | Why | Use instead |
| ------- | --- | ----------- |
| `text-[11px]`, `text-[13px]`, `text-[22px]` | Random type | `text-caption`, `text-body`, `text-display` |
| `text-xs` / `text-sm` / `text-base` (default Tailwind names) | Ambiguous vs our scale | Semantic steps below |
| `text-[#…]`, `bg-[#…]`, `border-[#…]` | Breaks theme | `text-fg`, `bg-surface`, … |
| `dark:bg-…` for theme surfaces | Duplicates CSS | Semantic `bg-*` tokens |
| Two classes for one property (`rounded-md` + `rounded-lg`) | `cn` does not merge; the winner is CSS order | A variant/size prop on the primitive |
| Inline hex for chrome | Not theme-aware | Tokens only |
| New font families in components | Brand drift | `font-sans` / `font-mono` |

**Allowed exception:** dynamic connection identity via `--connection-color` + `conn-tile` / `conn-tint` / `bg-conn` (never for general chrome).

---

## 2. Theme architecture

```
<html data-theme="dark|light" data-density="compact|comfortable">
        │
        ▼
tokens.css          ← semantic values per theme (the only layer themes touch)
        │
        ▼
app.css @theme      ← exposes Tailwind utilities (once) + design utilities
        │
        ▼
Components          ← semantic classes only
```

| Theme | Scheme (`data-theme`) | `data-palette` | Ramps |
| ----- | --------------------- | -------------- | ----- |
| **Night** | `dark` | — | Graphite × Mint |
| **Abyss** | `dark` | `abyss` | Navy × Iris |
| **Day** | `light` | — | Graphite × Mint (accent step 700 so white text passes 4.5:1) |
| **Linen** | `light` | `linen` | Warm paper × Ember |

Settings › Appearance picks the **mode** (Light · Dark · System) and one theme
of each scheme (`appPrefs.darkTheme`, `appPrefs.lightTheme`); System swaps
between the two picks with the OS. `data-theme` stays `dark | light`, so the
`dark:` variant, the editor and syntax inks follow the scheme. `app.html`
applies the saved theme before first paint.

Adding a theme: a `[data-palette='…']` block in `tokens.css`, an entry in
`app/themes.ts`, and its id in that scheme's type. Keep `--warn` on amber in
every theme so PROD always reads as PROD.

---

## 3. Color language

### 3.1 Surfaces

| Class | Token | Role |
| ----- | ----- | ---- |
| `bg-app` | `--bg` | App canvas, behind everything |
| `bg-side` | `--side` | Sidebars |
| `bg-surface` | `--surface` | Cards, panels |
| `bg-raised` | `--raised` | Popovers, menus, modals, toasts |
| `bg-field` | `--field` | Input wells |
| `bg-hover` | `--s2` | Hover, subtle fill, selected row |
| `bg-pressed` | `--s3` | Pressed, switch track |
| `bg-scrim` | `--scrim` | Modal backdrop |
| `bg-accent` / `bg-accent-hover` | `--accent` | Primary action |
| `bg-accent-soft` | `--accent-soft` | Selected fills, focus halo, ok state |

`bg-selected` (`--selected`, accent at 18%) marks selected text and rows in
the editor and grids.

### 3.2 Text

| Class | Token | Role |
| ----- | ----- | ---- |
| `text-fg` | `--text` | Primary text |
| `text-fg-secondary` | `--text-2` | Secondary text, labels |
| `text-fg-muted` | `--text-3` | Tertiary, placeholders. **The 4.5:1 floor** — never lighter |
| `text-on-accent` | `--on-accent` | Text on solid accent (`text-fg-inverse` is the old name) |
| `text-on-danger` | `--on-danger` | Text on solid danger |
| `text-accent-text` | `--accent-text` | Accent as text, links, ok messages |

### 3.3 Borders & focus

| Class | Token | Role |
| ----- | ----- | ---- |
| `border-line` | `--line` | Hairline dividers, card borders |
| `border-line-strong` | `--line-2` | Control borders |
| `border-focus` | `--accent` | Focused field border |
| `outline-ring` | `--ring` | `:focus-visible` outline (2px, offset 2) |
| `focus-field` | — | Field focus: accent border + 3px `accent-soft` halo |

### 3.4 Status

| Tone | Text | Soft fill | Meaning |
| ---- | ---- | --------- | ------- |
| accent | `text-accent-text` | `bg-accent-soft` | Connected, ok |
| warning | `text-warning` | `bg-warn-soft` | **Production only**, caution |
| danger | `text-danger` | `bg-danger-soft` | Delete, errors |
| info | `text-info` | `bg-info-soft` | Read-only, info |
| violet | `text-violet` | `bg-violet-soft` | Staging |

Badges (`Pill uppercase`) and banners (`Alert`) pair the soft fill with a 1px
ring in the tone (`badge`, `ring-tint`; set `--tint` to override the ring).

### 3.5 Connection colours (identity only)

Twelve hues tuned to the same lightness (`model.ts` `CONNECTION_COLORS`, ids are
stored): Rose, Coral, Amber, Lime, Green, Teal, Sky, Blue, Indigo, Violet,
Magenta, Slate. Tints are mixed at runtime: `conn-tile` is 16% fill, 30% ring,
text 72% toward `--text`, so it reads on every theme.

**Never** use connection hues for primary buttons or general chrome.

### 3.6 Syntax highlighting (`syntax.*`)

CodeMirror and any code chrome use `--syntax-*` (both themes in `tokens.css`).
Do not reuse `--accent` or connection hues for syntax.

### 3.7 Mascot

`--fur`, `--ear`, `--muzzle`, `--ink`, `--mascot-tag`, `--mascot-tongue`, read by `Mascot` only.

---

## 4. Typography language

### 4.1 Families

| Class | Use |
| ----- | --- |
| `font-sans` | **Geist** — UI chrome (default, `-0.006em`) |
| `font-mono` | **Geist Mono** — data: hosts, ports, SQL, JSON, grid cells, marks (`-0.01em`, tabular numbers) |

### 4.2 Size scale (closed set)

| Class | Size / line | Use |
| ----- | ----------- | --- |
| `text-micro` | 10 / 12 | Tiny mono marks |
| `text-caption` | 11 / 14 | Meta, mono sublines, shortcuts |
| `label-caps` | 11 / 14 · 600 · +8% caps | Section titles, fact labels (utility, includes colour) |
| `text-label` | 12 / 16 · 500 | Field labels, hints, secondary UI |
| `text-data` | 12.5 / 20 | Mono data values |
| `text-body` | 13 / 20 | **Default UI body**, inputs, buttons, list names |
| `text-body-lg` | 14 / 21 | Card titles, sidebar brand, page descriptions |
| `text-heading` | 15 / 22 · 600 | Dialog titles, section headings |
| `text-title` | 20 / 28 · 600 | Minor page headings, empty states |
| `text-display` | 28 / 32 · 600 · −2.5% | Page / connection titles |

### 4.3 Weight scale (closed set)

`font-normal` (400) body · `font-medium` (500) names, buttons, labels ·
`font-semibold` (600) titles, badges · `font-bold` (700) rare.

### 4.4 Recipes

| Recipe | Classes |
| ------ | ------- |
| Page title | `text-display font-semibold tracking-tight text-fg` |
| Section title | `label-caps` |
| Field label | `text-label font-medium text-fg-secondary` |
| Body | `text-body text-fg` |
| Hint | `text-label text-fg-muted` |
| Mono subline | `font-mono text-caption text-fg-muted` |
| Mono value | `font-mono text-data text-fg` |

---

## 5. Spacing language

4px rhythm, Tailwind spacing scale only: `0.5` 2 · `1` 4 · `1.5` 6 · `2` 8 · `3` 12 ·
`4` 16 · `5` 20 · `6` 24 · `8` 32 · `10` 40 · `12` 48 · `16` 64.

Layout widths:

| Class | Use |
| ----- | --- |
| `w-sidebar` | Left rail (`var(--sidebar-width)`) |
| `max-w-panel-sm` | Empty states (~420px) |
| `max-w-panel-md` | Forms (~720px) |
| `max-w-panel-lg` | Detail (~880px) |
| `max-w-panel-xl` | Home connection detail (980px) |
| `max-w-prose-ui` | Descriptions (~52ch) |
| `h-modal`, `max-h-modal-sm · md · lg` | Dialog heights |

---

## 6. Radius language

| Class | Value | Use |
| ----- | ----- | --- |
| `rounded-xs` | 4px | kbd, badges, marks |
| `rounded-sm` | 6px | Small chips |
| `rounded-md` | 8px | Buttons, inputs, menu items |
| `rounded-lg` | 12px | Cards, banners, toasts, menus |
| `rounded-xl` | 16px | Modals, the hero connection tile |
| `rounded-full` | pill | Switches, swatches, status dots |

---

## 7. Elevation

Depth by light, not by colour.

| Level | Classes | Use |
| ----- | ------- | --- |
| e0 flat | `bg-surface border border-line` | Panels, cards, rows |
| e1 raised | `shadow-sm` (`--shadow-1`) | Buttons |
| e2 popover | `bg-raised shadow-md` (`--shadow-2`) | Menus, selects, bubbles |
| e3 modal | `bg-raised shadow-lg` (`--shadow-3`) + `bg-scrim` | Dialogs, toasts |

---

## 8. Control sizes

| Class | Height | Use |
| ----- | ------ | --- |
| `h-control-sm` / `size-control-sm` | 28px | Small buttons, icon buttons |
| `h-control` / `size-control` | 32px | **Default** buttons |
| `h-field` | 36px | **Default** inputs, selects, password fields |
| `h-control-lg` / `size-control-lg` | 40px | The one hero action (Connect) |
| `h-topbar` | 52px | Sidebar header, page bar |
| `h-statusbar` | 34px | Workspace status footers |

Hit targets ≥ 28px. Input padding `px-3`; icon insets via `Input inset`.

---

## 9. Motion

| Token | Value | Use |
| ----- | ----- | --- |
| `--duration-fast` | 120ms | Hover, press |
| `--duration-base` | 180ms | Tabs, fades |
| `--duration-slow` | 280ms | Modals, panels |
| `--duration-spring` | 420ms | Switches, swatches, the dog |
| `ease-standard` | `cubic-bezier(.2,0,0,1)` | Default |
| `ease-emphasized` | `cubic-bezier(.05,.7,.1,1)` | Large surfaces |
| `ease-spring` | `cubic-bezier(.34,1.56,.64,1)` | **Only** things the user flips |

| Utility / helper | Use |
| ---------------- | --- |
| `animate-rise` | Enter: 6px rise + fade |
| `stagger` | Children rise 40ms apart (page sections) |
| `dot-live` | Live connection halo |
| `overlayFade` / `modalPanel` / `popover` / `toastSlide` / `fadeRise` | Svelte `transition:` enter/exit |

Animate `transform` and `opacity`. Press = `scale(.97)` (icon buttons `.92`).
All motion respects `prefers-reduced-motion`.

---

## 10. Component recipes (use primitives)

| Need | Primitive | Notes |
| ---- | --------- | ----- |
| Button | `Button` | `variant`: primary · secondary · ghost (no border) · danger (solid, dialog confirm) · danger-ghost (red text). `size`: sm 28 · md 32 · lg 40 |
| Icon button | `IconButton` | `variant`: quiet (bare glyph) · default · ghost · accent (soft, solid on hover) |
| Field | `Field` + `Input` / `PasswordInput` / `Select` / `Textarea` | `Input invalid` for errors |
| Card | `Card` | e0 surface, `label-caps` title |
| Status | `StatusDot` + `Pill` | `Pill uppercase` tones: warning PROD · violet Staging · neutral Local · info RO; `compact` in list rows |
| Banner | `Alert` | Soft tone + ring + glyph; `banner` for page-level notices |
| Busy | `Spinner` | `size`: xs (inline text) · sm · md |
| Engine mark | `EngineBadge` | Mono mark |
| Connection identity | `ConnectionTile` (connections) | Engine mark in the connection colour; sm 28 · lg 56 |
| Mascot | `Mascot` | `mood`: idle · alert (PROD in view) · happy (test passed, petted) · sleep · tilt; `zoom`, `playful` |
| Toast | `Toast` | `bg-raised shadow-lg`, tone glyph |
| Empty | `EmptyState` | `text-title` + body + next action |
| Modal | `Modal` | `layout`: inset · chrome · bare. Custom headers pass `labelledby` |
| Shortcut | `kbd` / `Kbd` | 20px mono chip with a 2px bottom border |

**Variants customize look; `class` only for layout** (`w-full`, `col-span-full`, `mt-3`).

---

## 11. Z-index scale

`z-10` sticky bars and corner decor · `z-20` the page bar · `z-50` menus, toasts, dialogs.

---

## 12. Layout shells

| Region | Classes (reference) |
| ------ | ------------------- |
| App root | `flex h-screen w-screen overflow-hidden bg-app` |
| Sidebar | `flex h-full flex-col border-r border-line bg-side` |
| Page bar | `sticky top-0 h-topbar border-b border-line bg-app-blur` (breadcrumb) |
| Content | centered column, `max-w-panel-*`, sections in `stagger` |

---

## 13. Atmosphere

Quiet surfaces: no grain, no gradient washes; the canvas is plain `bg-app`. Personality lives in the mascot and its small, skippable easter eggs
(logo zoomies, treat search, the corner peek), never in the way of work.

---

## 14. Accessibility

- Focus: `:focus-visible` draws a 2px `--ring` outline; fields use `focus-field`
- Contrast: body text ≥ 4.5:1 on every theme; `text-fg-muted` is the floor
- Hit targets ≥ 28px
- Status is always text or a glyph plus colour
- Icon-only buttons carry `aria-label` (`IconButton label`)
- Selection: chrome is `user-select: none`; data values opt back in with `select-text`

---

## 15. PR / review checklist

- [ ] No arbitrary font sizes or hex colours in components
- [ ] Type uses the §4.2 scale only
- [ ] Colours from §3; amber only for production
- [ ] Controls use `h-control*` / `h-field`
- [ ] Night **and** Day checked
- [ ] New patterns added to this doc

---

## 16. Extending the language

1. Propose the token here (name, value, use).
2. Add its value to every theme block in `tokens.css`.
3. Map it in `app.css` `@theme inline`.
4. Use it in primitives first.

---

## 17. Quick reference card

```txt
Type:     text-micro | text-caption | label-caps | text-label | text-data | text-body | text-body-lg | text-heading | text-title | text-display
Surface:  bg-app | bg-side | bg-surface | bg-raised | bg-field | bg-hover | bg-pressed | bg-scrim
Text:     text-fg | text-fg-secondary | text-fg-muted | text-on-accent | text-accent-text
Status:   accent (ok) | warning (PROD) | danger | info (RO) | violet (staging) — each with a *-soft fill
Border:   border-line | border-line-strong | focus-field | outline-ring
Control:  h-control-sm 28 | h-control 32 | h-field 36 | h-control-lg 40
Radius:   rounded-xs 4 | sm 6 | md 8 | lg 12 | xl 16 | full
Shadow:   shadow-sm (e1) | shadow-md (e2) | shadow-lg (e3)
Font:     font-sans (Geist) | font-mono (Geist Mono)
Accent:   mint #3ecf9b (Night) / #087f56 (Day) — never a connection hue
```

---

*If it’s not on this card, don’t ship it — extend the language first.*
