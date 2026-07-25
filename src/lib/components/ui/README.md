# UI primitives

Implementations of [`docs/DESIGN_LANGUAGE.md`](../../../../docs/DESIGN_LANGUAGE.md).

## Rules

1. Use **only** design-language classes (`text-body`, `bg-panel`, `h-control`, …).
2. **Never** `text-[13px]`, `text-xs`, hex colors, or random `h-[34px]`.
3. Variants via props; `class` only for layout (`w-full`, `mt-3`, `col-span-full`).
4. Theme is free: semantic colors read CSS variables under `[data-theme]`.

## Type scale

`text-micro` · `text-caption` · `text-label` · `text-body` · `text-body-lg` · `text-title` · `text-display`

## Import

```ts
import { Button, Field, Input } from "$lib/components/ui";
import { cn } from "$lib/utils/cn";
```
