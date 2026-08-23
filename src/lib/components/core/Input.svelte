<script lang="ts">
  import type { HTMLInputAttributes } from "svelte/elements";
  import { cn } from "$lib/utils/cn";

  interface Props extends Omit<HTMLInputAttributes, "size"> {
    value?: string | number | null;
    inputSize?: "sm" | "md";
    /** Extra inset for a leading / trailing icon inside the field. */
    inset?: "none" | "start" | "end" | "both";
  }

  let {
    value = $bindable(""),
    inputSize = "md",
    inset = "none",
    class: className = "",
    type = "text",
    spellcheck = false,
    autocomplete = "off",
    ...rest
  }: Props = $props();

  const pad = $derived(
    inset === "start"
      ? "pl-7 pr-2.5"
      : inset === "end"
        ? "pl-2.5 pr-7"
        : inset === "both"
          ? "px-7"
          : "px-2.5",
  );
</script>

<input
  class={cn(
    "w-full cursor-text rounded-md border border-line bg-input text-fg placeholder:text-fg-muted",
    pad,
    "focus:outline-none focus-visible:border-focus focus-visible:ring-2 focus-visible:ring-accent-muted",
    "disabled:opacity-55",
    inputSize === "sm" ? "h-control-sm text-label" : "h-control text-body",
    type === "number" &&
      "[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none",
    className,
  )}
  {type}
  {spellcheck}
  {autocomplete}
  bind:value
  {...rest}
/>
