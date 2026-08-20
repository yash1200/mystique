# Core primitives

Implementations of [`docs/DESIGN_LANGUAGE.md`](../../../../docs/DESIGN_LANGUAGE.md).

Generic controls only — Button, Modal, Input, Select, and the rest.  
Product chrome (settings, brand, shortcuts, engine marks) lives in `components/home` or `components/workspace`.

## Rules

1. Use **only** design-language classes (`text-body`, `bg-panel`, `h-control`, …).
2. **Never** `text-[13px]`, `text-xs`, hex colors, or random `h-[34px]`.
3. Variants via props; `class` only for layout (`w-full`, `mt-3`, `col-span-full`).
4. Theme is free: semantic colors read CSS variables under `[data-theme]`.

## Import

```ts
import { Button, Field, Input, Select, StatusBar, Pager } from "$lib/components/core";
import type { SelectOption } from "$lib/components/core";
import { SettingsModal, BrandMark } from "$lib/components/home";
```

## Select

Custom dropdown (not native `<select>`) so menus match the design language on every OS.

```svelte
<script lang="ts">
  import { Select } from "$lib/components/core";

  let env = $state("local");
  const options = [
    { value: "local", label: "Local" },
    { value: "staging", label: "Staging" },
    { value: "production", label: "Production" },
  ];
</script>

<Select bind:value={env} {options} />
```
