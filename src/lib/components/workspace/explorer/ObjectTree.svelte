<script lang="ts">
  import type { DatabaseMeta, TableMeta } from "$lib/types/workspace";
  import { cn } from "$lib/utils/cn";
  import CatalogIcon from "../CatalogIcon.svelte";

  interface Props {
    objectLabel: string;
    groupLabel: string;
    isTree: boolean;
    selectedDatabase: string;
    currentDb: DatabaseMeta | null;
    totalCount: number;
    query: string;
    activeTableId: string | null;
    groups: { key: string; items: TableMeta[] }[];
    flatItems: TableMeta[];
    isExpanded: (key: string) => boolean;
    onToggleGroup: (key: string) => void;
    onopen: (table: TableMeta) => void;
  }

  let {
    objectLabel,
    groupLabel,
    isTree,
    selectedDatabase,
    currentDb,
    totalCount,
    query,
    activeTableId,
    groups,
    flatItems,
    isExpanded,
    onToggleGroup,
    onopen,
  }: Props = $props();
</script>

<div
  class="flex-1 overflow-auto px-1.5 py-2"
  role="tree"
  aria-label="{objectLabel} in {selectedDatabase}"
>
  {#if !currentDb}
    <p class="m-2 text-caption leading-relaxed text-fg-muted">
      No database selected.
    </p>
  {:else if totalCount === 0}
    <p class="m-2 text-caption leading-relaxed text-fg-muted">
      No {objectLabel} match
      {#if query.trim()}
        for “{query.trim()}”
      {:else}
        in <span class="font-mono text-fg-secondary">{currentDb.name}</span>
      {/if}.
    </p>
  {:else if isTree}
    {#each groups as group (group.key)}
      {@const expanded = isExpanded(group.key)}
      <div class="mb-0.5" role="group" aria-label="{groupLabel} {group.key}">
        <button
          type="button"
          class={cn(
            "flex w-full items-center gap-1.5 rounded-md border-none px-1.5 py-1.5 text-left transition-colors",
            "bg-transparent text-fg hover:bg-hover",
          )}
          aria-expanded={expanded}
          onclick={() => onToggleGroup(group.key)}
        >
          <CatalogIcon
            name="chevron"
            class={cn(
              "transition-transform duration-150",
              expanded ? "rotate-90" : "rotate-0",
            )}
          />
          <span
            class="grid size-5 shrink-0 place-items-center rounded-sm border border-line bg-elevated"
          >
            <CatalogIcon name="schema" />
          </span>
          <span class="min-w-0 flex-1 truncate text-label font-semibold text-fg">
            {group.key}
          </span>
          <span
            class="shrink-0 rounded-sm bg-input px-1.5 py-0.5 font-mono text-micro text-fg-muted tabular-nums"
          >
            {group.items.length}
          </span>
        </button>

        {#if expanded}
          <ul class="m-0 ml-3.5 list-none border-l border-line py-0.5 pl-2" role="group">
            {#each group.items as t (t.id)}
              {@const active = activeTableId === t.id}
              <li role="treeitem" aria-selected={active}>
                <button
                  type="button"
                  class={cn(
                    "flex w-full items-center gap-2 rounded-md border-none px-2 py-1.5 text-left transition-colors",
                    active
                      ? "bg-accent-muted text-accent"
                      : "bg-transparent text-fg hover:bg-hover",
                  )}
                  ondblclick={() => onopen(t)}
                >
                  <CatalogIcon name={t.kind} {active} />
                  <span class="min-w-0 flex-1 truncate text-label font-medium">
                    {t.name}
                  </span>
                  {#if t.kind === "view"}
                    <span
                      class={cn(
                        "shrink-0 rounded-sm px-1 py-px text-micro font-medium tracking-wide uppercase",
                        active ? "bg-accent/20 text-accent" : "bg-input text-fg-muted",
                      )}
                    >
                      view
                    </span>
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/each}
  {:else}
    <div
      class="mb-1 px-2 py-1 text-caption font-semibold tracking-wide text-fg-muted uppercase"
    >
      {objectLabel}
      <span class="ml-1 font-mono font-normal normal-case tabular-nums">
        {flatItems.length}
      </span>
    </div>
    <ul class="m-0 list-none p-0" role="group">
      {#each flatItems as t (t.id)}
        {@const active = activeTableId === t.id}
        <li role="treeitem" aria-selected={active}>
          <button
            type="button"
            class={cn(
              "flex w-full items-center gap-2 rounded-md border-none px-2 py-1.5 text-left transition-colors",
              active
                ? "bg-accent-muted text-accent"
                : "bg-transparent text-fg hover:bg-hover",
            )}
            ondblclick={() => onopen(t)}
          >
            <span
              class={cn(
                "grid size-6 shrink-0 place-items-center rounded-md border",
                active
                  ? "border-accent/35 bg-accent/15"
                  : "border-line bg-elevated",
              )}
            >
              <CatalogIcon name={t.kind} {active} />
            </span>
            <span class="min-w-0 flex-1 truncate text-label font-medium">
              {t.name}
            </span>
            {#if t.kind === "view"}
              <span
                class={cn(
                  "shrink-0 rounded-sm px-1 py-px text-micro font-medium tracking-wide uppercase",
                  active ? "bg-accent/20 text-accent" : "bg-input text-fg-muted",
                )}
              >
                view
              </span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
