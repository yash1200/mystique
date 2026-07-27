<script lang="ts">
  import type { DatabaseMeta, TableMeta } from "$lib/types/workspace";
  import { catalogPresentation } from "$lib/types/workspace";
  import { IconButton, Input, Select } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";
  import CatalogIcon from "./CatalogIcon.svelte";

  interface Props {
    engine: string;
    databases: DatabaseMeta[];
    selectedDatabase: string;
    activeTableId: string | null;
    refreshing?: boolean;
    /** Building live ER / model diagram. */
    modelLoading?: boolean;
    onselectdatabase: (name: string) => void;
    onrefresh: () => void;
    oncreatedatabase: (name: string) => void;
    onopen: (table: TableMeta) => void;
    /** Open live ER / model diagram for the selected database (all engines). */
    onopenmodel?: () => void;
  }

  let {
    engine,
    databases,
    selectedDatabase,
    activeTableId,
    refreshing = false,
    modelLoading = false,
    onselectdatabase,
    onrefresh,
    oncreatedatabase,
    onopen,
    onopenmodel,
  }: Props = $props();

  let query = $state("");
  let collapsed = $state<Set<string>>(new Set());
  let creating = $state(false);
  let newDbName = $state("");
  let createError = $state<string | null>(null);

  const presentation = $derived(catalogPresentation(engine));
  const isTree = $derived(presentation.layout === "schema-tree");

  const currentDb = $derived(
    databases.find((d) => d.name === selectedDatabase) ?? databases[0] ?? null,
  );

  const tables = $derived(currentDb?.tables ?? []);

  const dbOptions = $derived(
    databases.map((d) => ({
      value: d.name,
      label: d.meta ? `${d.name}  ·  ${d.meta}` : d.name,
    })),
  );

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tables;
    return tables.filter(
      (t) =>
        t.name.toLowerCase().includes(q) ||
        t.schema.toLowerCase().includes(q) ||
        t.id.toLowerCase().includes(q) ||
        t.kind.toLowerCase().includes(q),
    );
  });

  const groups = $derived.by(() => {
    if (!isTree) return [] as { key: string; items: TableMeta[] }[];
    const map = new Map<string, TableMeta[]>();
    for (const t of filtered) {
      const key = t.schema || "public";
      const list = map.get(key) ?? [];
      list.push(t);
      map.set(key, list);
    }
    return [...map.entries()]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([key, items]) => ({
        key,
        items: [...items].sort((a, b) => {
          const rank = (k: string) =>
            k === "table" ? 0 : k === "collection" ? 1 : 2;
          const d = rank(a.kind) - rank(b.kind);
          return d !== 0 ? d : a.name.localeCompare(b.name);
        }),
      }));
  });

  const flatItems = $derived.by(() => {
    if (isTree) return [] as TableMeta[];
    return [...filtered].sort((a, b) => {
      const rank = (k: string) => (k === "table" ? 0 : k === "view" ? 1 : 2);
      const d = rank(a.kind) - rank(b.kind);
      return d !== 0 ? d : a.name.localeCompare(b.name);
    });
  });

  const totalCount = $derived(filtered.length);

  function toggleGroup(key: string) {
    const next = new Set(collapsed);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    collapsed = next;
  }

  function isExpanded(key: string): boolean {
    if (query.trim()) return true;
    if (activeTableId) {
      const hit = filtered.some(
        (t) => t.id === activeTableId && (t.schema || "public") === key,
      );
      if (hit) return true;
    }
    return !collapsed.has(key);
  }

  function formatCount(n: number): string {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 10_000) return `${Math.round(n / 1000)}k`;
    if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
    return n.toLocaleString();
  }

  function openCreate() {
    if (!presentation.canCreateDatabase) return;
    creating = true;
    newDbName = "";
    createError = null;
  }

  function cancelCreate() {
    creating = false;
    newDbName = "";
    createError = null;
  }

  function submitCreate(e: Event) {
    e.preventDefault();
    const name = newDbName.trim();
    if (!name) {
      createError = "Enter a database name.";
      return;
    }
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) {
      createError = "Use letters, numbers, underscore; start with a letter.";
      return;
    }
    if (databases.some((d) => d.name === name)) {
      createError = "A database with this name already exists.";
      return;
    }
    oncreatedatabase(name);
    creating = false;
    newDbName = "";
    createError = null;
  }

  function onDbChange(value: string) {
    query = "";
    collapsed = new Set();
    onselectdatabase(value);
  }
</script>

<aside
  class="flex h-full w-60 shrink-0 flex-col border-r border-line bg-panel"
  aria-label="Database objects"
>
  <!-- Database picker + actions (replaces connection header) -->
  <div class="shrink-0 space-y-2 border-b border-line px-2 py-2">
    <div class="flex items-center gap-1.5">
      <div class="min-w-0 flex-1">
        <span class="sr-only">{presentation.databaseLabel}</span>
        <Select
          id="workspace-database"
          value={selectedDatabase}
          options={dbOptions}
          placeholder="Select database…"
          inputSize="sm"
          onchange={onDbChange}
        />
      </div>
      <IconButton
        label="Refresh databases"
        size="sm"
        variant="ghost"
        disabled={refreshing}
        onclick={onrefresh}
      >
        <svg
          class={cn("size-3.5", refreshing && "animate-spin")}
          viewBox="0 0 16 16"
          fill="none"
          aria-hidden="true"
        >
          <path
            d="M13.5 8A5.5 5.5 0 1 1 11.2 3.4"
            stroke="currentColor"
            stroke-width="1.4"
            stroke-linecap="round"
          />
          <path
            d="M13.5 2.5v3.2h-3.2"
            stroke="currentColor"
            stroke-width="1.4"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </IconButton>
      {#if presentation.canCreateDatabase}
        <IconButton
          label="Create database"
          size="sm"
          variant="ghost"
          onclick={openCreate}
        >
          <svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path
              d="M8 3.25v9.5M3.25 8h9.5"
              stroke="currentColor"
              stroke-width="1.4"
              stroke-linecap="round"
            />
          </svg>
        </IconButton>
      {/if}
    </div>

    {#if creating}
      <form
        class="rounded-md border border-line bg-elevated p-2"
        onsubmit={submitCreate}
      >
        <label class="mb-1 block text-caption font-medium text-fg-secondary" for="new-db-name">
          New database
        </label>
        <Input
          id="new-db-name"
          bind:value={newDbName}
          placeholder="my_database"
          inputSize="sm"
          autocomplete="off"
          class="font-mono"
        />
        {#if createError}
          <p class="mt-1 text-caption text-danger">{createError}</p>
        {/if}
        <div class="mt-2 flex justify-end gap-1.5">
          <button
            type="button"
            class="h-control-sm rounded-md border border-line bg-transparent px-2 text-label text-fg-secondary hover:bg-hover"
            onclick={cancelCreate}
          >
            Cancel
          </button>
          <button
            type="submit"
            class="h-control-sm rounded-md border border-transparent bg-accent px-2 text-label font-medium text-fg-inverse hover:bg-accent-hover"
          >
            Create
          </button>
        </div>
      </form>
    {/if}

    <Input
      type="search"
      placeholder={presentation.filterPlaceholder}
      bind:value={query}
      inputSize="sm"
    />

    {#if onopenmodel}
      <button
        type="button"
        class={cn(
          "flex w-full items-center justify-center gap-1.5 rounded-md border px-2 py-1.5 text-label font-medium transition-colors",
          "border-line bg-elevated text-fg hover:border-accent hover:bg-accent-muted hover:text-accent",
          "disabled:cursor-not-allowed disabled:opacity-60",
        )}
        disabled={modelLoading || !currentDb}
        title="Show full ER / model diagram for this database"
        onclick={() => onopenmodel?.()}
      >
        <svg class="size-3.5 shrink-0" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <rect x="1.5" y="2" width="5" height="4.5" rx="0.8" stroke="currentColor" stroke-width="1.2" />
          <rect x="9.5" y="2" width="5" height="4.5" rx="0.8" stroke="currentColor" stroke-width="1.2" />
          <rect x="5.5" y="9.5" width="5" height="4.5" rx="0.8" stroke="currentColor" stroke-width="1.2" />
          <path d="M4 6.5v1.5M12 6.5v1.5M8 9.5V8" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
        </svg>
        {#if modelLoading}
          Building diagram…
        {:else}
          ER diagram
        {/if}
      </button>
    {/if}
  </div>

  <div
    class="flex-1 overflow-auto px-1.5 py-2"
    role="tree"
    aria-label="{presentation.objectLabel} in {selectedDatabase}"
  >
    {#if !currentDb}
      <p class="m-2 text-caption leading-relaxed text-fg-muted">
        No database selected.
      </p>
    {:else if totalCount === 0}
      <p class="m-2 text-caption leading-relaxed text-fg-muted">
        No {presentation.objectLabel} match
        {#if query.trim()}
          for “{query.trim()}”
        {:else}
          in <span class="font-mono text-fg-secondary">{currentDb.name}</span>
        {/if}.
      </p>
    {:else if isTree}
      {#each groups as group (group.key)}
        {@const expanded = isExpanded(group.key)}
        <div
          class="mb-0.5"
          role="group"
          aria-label="{presentation.groupLabel} {group.key}"
        >
          <button
            type="button"
            class={cn(
              "flex w-full items-center gap-1.5 rounded-md border-none px-1.5 py-1.5 text-left transition-colors",
              "bg-transparent text-fg hover:bg-hover",
            )}
            aria-expanded={expanded}
            onclick={() => toggleGroup(group.key)}
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
            <ul
              class="m-0 ml-3.5 list-none border-l border-line py-0.5 pl-2"
              role="group"
            >
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
                    onclick={() => onopen(t)}
                  >
                    <CatalogIcon name={t.kind} {active} />
                    <span class="min-w-0 flex-1 truncate text-label font-medium">
                      {t.name}
                    </span>
                    {#if t.kind === "view"}
                      <span
                        class={cn(
                          "shrink-0 rounded-sm px-1 py-px text-micro font-medium tracking-wide uppercase",
                          active
                            ? "bg-accent/20 text-accent"
                            : "bg-input text-fg-muted",
                        )}
                      >
                        view
                      </span>
                    {/if}
                    <span
                      class={cn(
                        "shrink-0 font-mono text-caption tabular-nums",
                        active ? "text-accent/80" : "text-fg-muted",
                      )}
                      title="{t.rowCount.toLocaleString()} rows"
                    >
                      {formatCount(t.rowCount)}
                    </span>
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
        {presentation.objectLabel}
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
              onclick={() => onopen(t)}
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
              <span
                class={cn(
                  "shrink-0 font-mono text-caption tabular-nums",
                  active ? "text-accent/80" : "text-fg-muted",
                )}
                title="{t.rowCount.toLocaleString()} rows"
              >
                {formatCount(t.rowCount)}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <footer
    class="flex h-statusbar shrink-0 items-center border-t border-line bg-elevated px-2.5 text-caption text-fg-muted"
  >
    <div class="min-w-0 truncate">
      {#if currentDb}
        <span class="font-mono text-fg-secondary">{currentDb.name}</span>
        <span aria-hidden="true"> · </span>
      {/if}
      {totalCount}
      {presentation.objectLabel}
      {#if isTree && groups.length > 0}
        · {groups.length}
        {groups.length === 1
          ? presentation.groupLabel.toLowerCase()
          : `${presentation.groupLabel.toLowerCase()}s`}
      {/if}
    </div>
  </footer>
</aside>
