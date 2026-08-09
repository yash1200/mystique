# UI primitives

Implementations of [`docs/DESIGN_LANGUAGE.md`](../../../../docs/DESIGN_LANGUAGE.md).

## Rules

1. Use **only** design-language classes (`text-body`, `bg-panel`, `h-control`, …).
2. **Never** `text-[13px]`, `text-xs`, hex colors, or random `h-[34px]`.
3. Variants via props; `class` only for layout (`w-full`, `mt-3`, `col-span-full`).
4. Theme is free: semantic colors read CSS variables under `[data-theme]`.

## Type scale

`text-micro` · `text-caption` · `text-label` · `text-body` · `text-body-lg` · `text-title` · `text-display`

## Shared chrome

| Primitive | Use |
| --------- | --- |
| `StatusBar` | Footer with left / center / right snippets |
| `Pager` | ‹ n/m › inside a status bar |
| `AppSettings` | Theme, density, scale, remember-last |
| `AppSettingsPopover` | Gear trigger + settings panel |
| `EmptyState` | Centered title + description + actions |
| `BrandMark` | Jade lens logo |
| `Kbd` | Shortcut chip |
| `ContextMenu` | Right-click / action menu |

## Import

```ts
import { Button, Field, Input, Select, StatusBar, Pager } from "$lib/components/ui";
import type { SelectOption } from "$lib/components/ui";
import { cn } from "$lib/utils/cn";
```

## Select

Custom dropdown (not native `<select>`) so menus match the design language on every OS.

```svelte
<script lang="ts">
  import { Select } from "$lib/components/ui";

  let env = $state("local");
  const options = [
    { value: "local", label: "Local" },
    { value: "staging", label: "Staging" },
    { value: "production", label: "Production" },
  ];
</script>

<Select bind:value={env} {options} />
```
