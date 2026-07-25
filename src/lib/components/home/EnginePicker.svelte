<script lang="ts">
  import {
    selectableEngines,
    type EngineId,
    type EngineOption,
  } from "$lib/types/connection";
  import {
    Badge,
    Button,
    ContentPanel,
    PageHeader,
  } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";

  interface Props {
    /** Currently highlighted engine (keyboard / click preview). */
    selectedEngine?: EngineId | null;
    onselect: (engine: EngineId) => void;
    oncancel?: () => void;
    /** Show cancel when user already has connections. */
    showCancel?: boolean;
  }

  let {
    selectedEngine = null,
    onselect,
    oncancel,
    showCancel = false,
  }: Props = $props();

  const engines = $derived(selectableEngines());

  function familyLabel(opt: EngineOption): string {
    return opt.family === "document" ? "Document" : "Relational · SQL";
  }

  function onKeySelect(e: KeyboardEvent, id: EngineId) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onselect(id);
    }
  }
</script>

<ContentPanel size="lg" ariaLabel="Choose database engine">
  <PageHeader
    title="Choose a database"
    description="Pick the engine you want to work with. Mystique adapts Explorer, Query, and Model view to that engine — same chrome, honest structure."
  />

  <p class="mb-4 text-label font-semibold tracking-wider text-fg-muted uppercase">
    Available engines
  </p>

  <div
    class="grid grid-cols-1 gap-3 sm:grid-cols-3"
    role="listbox"
    aria-label="Database engines"
  >
    {#each engines as eng (eng.id)}
      {@const active = selectedEngine === eng.id}
      <button
        type="button"
        role="option"
        aria-selected={active}
        class={cn(
          "group flex min-h-[11rem] flex-col rounded-xl border bg-panel p-4 text-left transition-colors duration-150",
          "hover:border-line-strong hover:bg-hover",
          "focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-2",
          active
            ? "border-accent bg-accent-muted ring-1 ring-accent/40"
            : "border-line",
        )}
        onclick={() => onselect(eng.id)}
        onkeydown={(e) => onKeySelect(e, eng.id)}
      >
        <div class="mb-3 flex items-start justify-between gap-2">
          <span
            class={cn(
              "inline-flex h-9 min-w-9 items-center justify-center rounded-lg border font-mono text-label font-semibold",
              active
                ? "border-accent/40 bg-accent-muted text-accent"
                : "border-line bg-input text-fg-secondary",
            )}
          >
            {eng.mark}
          </span>
          <Badge tone={active ? "accent" : "neutral"} size="sm">
            {eng.queryLanguage}
          </Badge>
        </div>

        <h2 class="m-0 text-body-lg font-semibold text-fg">
          {eng.name}
        </h2>
        <p class="mt-1 text-caption text-fg-muted">
          {familyLabel(eng)}
        </p>
        <p class="mt-2 flex-1 text-body text-fg-secondary">
          {eng.description}
        </p>
        <p class="mt-3 text-caption text-fg-muted">
          Model view · {eng.modelViewLabel}
        </p>
      </button>
    {/each}
  </div>

  <p class="mt-5 text-caption text-fg-muted">
    More engines (MySQL, Cassandra, DynamoDB, …) will appear here as connectors ship.
    Connection profiles stay on this machine; live connect lands with each engine plugin.
  </p>

  {#if showCancel && oncancel}
    <div class="mt-5 flex justify-end">
      <Button variant="ghost" onclick={oncancel}>Cancel</Button>
    </div>
  {/if}
</ContentPanel>
