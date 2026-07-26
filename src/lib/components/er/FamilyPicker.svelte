<script lang="ts">
  import type { DataFamily } from "$lib/types/er";
  import { FAMILY_META } from "$lib/types/er";
  import { Button, ContentPanel, PageHeader } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";

  interface Props {
    onselect: (family: DataFamily) => void;
    onback: () => void;
  }

  let { onselect, onback }: Props = $props();

  const families: DataFamily[] = ["sql", "nosql", "redis"];
</script>

<ContentPanel size="lg" ariaLabel="Choose data family for ER">
  <PageHeader
    title="Create ER / model"
    description="Pick the kind of database you want to model. Tools and relationship types adapt to SQL, document NoSQL, or Redis. FE-only canvas for now — live metadata comes later."
  >
    {#snippet actions()}
      <Button variant="ghost" onclick={onback}>← Back to home</Button>
    {/snippet}
  </PageHeader>

  <p class="mb-4 text-label font-semibold tracking-wider text-fg-muted uppercase">
    Database family
  </p>

  <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
    {#each families as fam (fam)}
      {@const meta = FAMILY_META[fam]}
      <button
        type="button"
        class={cn(
          "flex min-h-[12rem] flex-col rounded-xl border border-line bg-panel p-5 text-left transition-colors duration-150",
          "hover:border-accent hover:bg-accent-muted/40",
          "focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-2",
        )}
        onclick={() => onselect(fam)}
      >
        <span
          class="mb-3 inline-flex h-8 min-w-8 items-center justify-center rounded-lg border border-line bg-input px-2 font-mono text-label font-semibold text-fg-secondary"
        >
          {meta.short}
        </span>
        <span class="text-body-lg font-semibold text-fg">{meta.title}</span>
        <span class="mt-2 flex-1 text-body text-fg-secondary">{meta.description}</span>
        <span class="mt-3 text-caption text-fg-muted">
          Nodes · {meta.nodeLabel} · Edges · {meta.edgeLabel}
        </span>
      </button>
    {/each}
  </div>
</ContentPanel>
