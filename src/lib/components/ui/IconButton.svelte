<script lang="ts">
  import type { HTMLButtonAttributes } from "svelte/elements";
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils/cn";

  type Variant = "default" | "accent" | "ghost";

  interface Props extends HTMLButtonAttributes {
    label: string;
    variant?: Variant;
    active?: boolean;
    size?: number;
    children?: Snippet;
  }

  let {
    label,
    variant = "default",
    active = false,
    size = 32,
    type = "button",
    class: className = "",
    children,
    ...rest
  }: Props = $props();
</script>

<button
  {type}
  class={cn(
    "inline-grid shrink-0 place-items-center rounded-md border p-0 text-base leading-none transition-colors",
    variant === "ghost" && "border-line bg-transparent text-fg",
    variant === "default" && "border-line bg-elevated text-fg",
    variant === "accent" && "border-line bg-elevated text-fg",
    !active && "hover:border-line-strong hover:bg-hover",
    active && "border-accent bg-accent-muted text-accent",
    className,
  )}
  style="width: {size}px; height: {size}px"
  title={label}
  aria-label={label}
  aria-pressed={active}
  {...rest}
>
  {@render children?.()}
</button>
