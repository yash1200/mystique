<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    description?: string;
    actions?: Snippet;
  }

  let { title, description, actions }: Props = $props();
</script>

<header class="mb-5 flex flex-wrap items-start justify-between gap-4">
  <div>
    <h1 class="m-0 text-pretty text-display font-semibold tracking-tight text-fg">
      {title}
    </h1>
    {#if description}
      <p
        class="mt-2 max-w-prose-ui text-body leading-relaxed text-fg-secondary"
      >
        {description}
      </p>
    {/if}
  </div>
  {#if actions}
    <div class="flex flex-wrap gap-2">
      {@render actions()}
    </div>
  {/if}
</header>
