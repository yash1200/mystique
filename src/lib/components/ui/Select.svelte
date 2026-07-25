<script lang="ts">
  import type { HTMLSelectAttributes } from "svelte/elements";
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils/cn";

  interface Props extends HTMLSelectAttributes {
    value?: string | number;
    inputSize?: "sm" | "md";
    children?: Snippet;
  }

  let {
    value = $bindable(""),
    inputSize = "md",
    class: className = "",
    children,
    ...rest
  }: Props = $props();
</script>

<select
  class={cn(
    "w-full rounded-md border border-line bg-input px-2.5 text-fg",
    "focus:border-focus focus:outline-none focus:ring-2 focus:ring-accent-muted",
    "disabled:opacity-55",
    inputSize === "sm" ? "h-control-sm text-label" : "h-control text-body",
    className,
  )}
  bind:value
  {...rest}
>
  {@render children?.()}
</select>
