<script lang="ts">
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils/cn";

  interface Props {
    label: string;
    hint?: string;
    error?: string;
    full?: boolean;
    htmlFor?: string;
    children?: Snippet;
  }

  let { label, hint, error, full = false, htmlFor, children }: Props = $props();
</script>

<div class={cn("flex min-w-0 flex-col gap-1.5", full && "col-span-full")}>
  <label class="text-label font-medium text-fg-secondary" for={htmlFor}>{label}</label>
  {@render children?.()}
  {#if error}
    <span class="text-caption text-danger" role="alert">{error}</span>
  {:else if hint}
    <span class="text-caption text-fg-muted">{hint}</span>
  {/if}
</div>
