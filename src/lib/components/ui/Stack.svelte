<script lang="ts">
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils/cn";

  interface Props {
    gap?: "sm" | "md" | "lg";
    children?: Snippet;
  }

  let { gap = "md", children }: Props = $props();
</script>

<div
  class={cn(
    "flex min-w-0 flex-col",
    gap === "sm" && "gap-2",
    gap === "md" && "gap-5",
    gap === "lg" && "gap-6",
  )}
>
  {@render children?.()}
</div>
