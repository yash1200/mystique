<script lang="ts">
  import type { HTMLTextareaAttributes } from "svelte/elements";
  import { cn } from "$lib/utils/cn";

  interface Props extends HTMLTextareaAttributes {
    value?: string;
  }

  let {
    value = $bindable(""),
    class: className = "",
    rows = 3,
    ...rest
  }: Props = $props();
</script>

<textarea
  class={cn(
    "min-h-14 w-full resize-y rounded-md border border-line bg-input px-2.5 py-2 text-body text-fg",
    "placeholder:text-fg-muted focus:outline-none focus-visible:border-focus focus-visible:ring-2 focus-visible:ring-accent-muted",
    className,
  )}
  {rows}
  bind:value
  {...rest}
></textarea>
