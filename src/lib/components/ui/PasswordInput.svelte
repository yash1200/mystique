<script lang="ts">
  import type { HTMLInputAttributes } from "svelte/elements";
  import { cn } from "$lib/utils/cn";

  interface Props extends Omit<HTMLInputAttributes, "size" | "type"> {
    value?: string | number | null;
    inputSize?: "sm" | "md";
  }

  let {
    value = $bindable(""),
    inputSize = "md",
    class: className = "",
    disabled = false,
    ...rest
  }: Props = $props();

  let visible = $state(false);

  const inputType = $derived(visible ? "text" : "password");
  const toggleLabel = $derived(visible ? "Hide password" : "Show password");
</script>

<div class={cn("relative w-full", className)}>
  <input
    class={cn(
      "w-full rounded-md border border-line bg-input pl-2.5 pr-16 text-fg placeholder:text-fg-muted",
      "focus:border-focus focus:outline-none focus:ring-2 focus:ring-accent-muted",
      "disabled:opacity-55",
      inputSize === "sm" ? "h-control-sm text-label" : "h-control text-body",
    )}
    type={inputType}
    bind:value
    {disabled}
    spellcheck="false"
    {...rest}
  />
  <button
    type="button"
    class={cn(
      "absolute top-1/2 right-1.5 -translate-y-1/2 rounded-sm border-none bg-transparent",
      "px-1.5 py-0.5 text-caption font-medium text-accent",
      "hover:bg-hover hover:underline",
      "focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus",
      "disabled:pointer-events-none disabled:opacity-55",
    )}
    aria-label={toggleLabel}
    title={toggleLabel}
    aria-pressed={visible}
    {disabled}
    onclick={() => {
      visible = !visible;
    }}
  >
    {visible ? "Hide" : "Show"}
  </button>
</div>
