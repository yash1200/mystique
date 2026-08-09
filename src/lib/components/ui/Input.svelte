<script lang="ts">
  import type { HTMLInputAttributes } from "svelte/elements";
  import { cn } from "$lib/utils/cn";

  interface Props extends Omit<HTMLInputAttributes, "size"> {
    value?: string | number | null;
    inputSize?: "sm" | "md";
  }

  let {
    value = $bindable(""),
    inputSize = "md",
    class: className = "",
    type = "text",
    ...rest
  }: Props = $props();
</script>

<input
  class={cn(
    "w-full cursor-text rounded-md border border-line bg-input px-2.5 text-fg placeholder:text-fg-muted",
    "focus:border-focus focus:outline-none focus:ring-2 focus:ring-accent-muted",
    "disabled:opacity-55",
    inputSize === "sm" ? "h-control-sm text-label" : "h-control text-body",
    type === "number" &&
      "[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none",
    className,
  )}
  {type}
  bind:value
  {...rest}
/>
