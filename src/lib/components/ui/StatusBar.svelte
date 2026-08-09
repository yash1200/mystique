<script lang="ts">
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils/cn";

  interface Props {
    left?: Snippet;
    center?: Snippet;
    right?: Snippet;
    class?: string;
    ariaLabel?: string;
  }

  let {
    left,
    center,
    right,
    class: className = "",
    ariaLabel = "Status",
  }: Props = $props();
</script>

<footer
  class={cn(
    "flex min-h-statusbar shrink-0 flex-wrap items-center gap-x-2 gap-y-1 border-t border-line bg-elevated px-2.5 py-0.5",
    className,
  )}
  aria-label={ariaLabel}
>
  <div
    class="flex min-w-[8rem] flex-1 items-center gap-1.5 overflow-hidden text-caption text-fg-secondary"
  >
    {@render left?.()}
  </div>
  <div class="flex shrink-0 items-center gap-1.5">
    {@render center?.()}
  </div>
  <div class="flex min-w-[8rem] flex-1 items-center justify-end gap-1">
    {@render right?.()}
  </div>
</footer>
