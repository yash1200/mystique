<script lang="ts">
  import type { HTMLButtonAttributes } from "svelte/elements";
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils/cn";

  type Variant = "default" | "accent" | "ghost";
  /** Matches Input/Button control scale from the design language. */
  type ControlSize = "sm" | "md" | "lg";

  interface Props extends HTMLButtonAttributes {
    label: string;
    variant?: Variant;
    active?: boolean;
    /** Design-language size; aligns with Input `inputSize`. */
    size?: ControlSize;
    children?: Snippet;
  }

  let {
    label,
    variant = "default",
    active = false,
    size = "md",
    type = "button",
    class: className = "",
    children,
    ...rest
  }: Props = $props();

  const sizes: Record<ControlSize, string> = {
    sm: "size-control-sm text-label",
    md: "size-control text-body",
    lg: "size-control-lg text-body-lg",
  };
</script>

<button
  {type}
  class={cn(
    "inline-grid shrink-0 place-items-center rounded-md border p-0 leading-none transition-colors",
    sizes[size],
    variant === "ghost" && "border-line bg-transparent text-fg",
    variant === "default" && "border-line bg-elevated text-fg",
    variant === "accent" && "border-line bg-elevated text-fg",
    !active && "hover:border-line-strong hover:bg-hover",
    active && "border-accent bg-accent-muted text-accent",
    className,
  )}
  title={label}
  aria-label={label}
  aria-pressed={active}
  {...rest}
>
  {@render children?.()}
</button>
