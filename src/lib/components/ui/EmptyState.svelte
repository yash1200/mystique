<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    description?: string;
    actions?: Snippet;
  }

  let { title, description, actions }: Props = $props();
</script>

<div class="mx-auto max-w-panel-sm px-6 py-[12vh] text-center">
  <h1 class="m-0 mb-2 text-title font-semibold text-fg">{title}</h1>
  {#if description}
    <p class="mb-4 text-body leading-relaxed text-fg-secondary">{description}</p>
  {/if}
  {#if actions}
    <div class="flex justify-center gap-2">
      {@render actions()}
    </div>
  {/if}
</div>
