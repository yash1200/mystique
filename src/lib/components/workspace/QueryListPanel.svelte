<script lang="ts">
  import { Input } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";
  import {
    formatRelativeTime,
    previewSql,
    type QueryHistoryEntry,
    type SavedQuery,
  } from "$lib/data/mock-queries";

  type Mode = "saved-queries" | "history";

  interface Props {
    mode: Mode;
    savedQueries: SavedQuery[];
    history: QueryHistoryEntry[];
    selectedId: string | null;
    onselect: (id: string) => void;
  }

  let { mode, savedQueries, history, selectedId, onselect }: Props = $props();

  let query = $state("");

  const title = $derived(
    mode === "saved-queries" ? "Saved queries" : "Query history",
  );
  const subtitle = $derived(
    mode === "saved-queries"
      ? "Named queries for this workspace"
      : "Recently executed statements",
  );

  const filteredSaved = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return savedQueries;
    return savedQueries.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        s.sql.toLowerCase().includes(q) ||
        (s.folder ?? "").toLowerCase().includes(q),
    );
  });

  const filteredHistory = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return history;
    return history.filter(
      (h) =>
        h.sql.toLowerCase().includes(q) ||
        (h.connectionName ?? "").toLowerCase().includes(q) ||
        (h.error ?? "").toLowerCase().includes(q),
    );
  });

  const savedGroups = $derived.by(() => {
    const map = new Map<string, SavedQuery[]>();
    for (const s of filteredSaved) {
      const key = s.folder?.trim() || "Unfiled";
      const list = map.get(key) ?? [];
      list.push(s);
      map.set(key, list);
    }
    return [...map.entries()].sort(([a], [b]) => a.localeCompare(b));
  });

  const count = $derived(
    mode === "saved-queries" ? filteredSaved.length : filteredHistory.length,
  );
</script>

<aside
  class="flex h-full w-60 shrink-0 flex-col border-r border-line bg-panel"
  aria-label={title}
>
  <header class="shrink-0 border-b border-line px-3 py-2.5">
    <div class="truncate text-label font-semibold text-fg">{title}</div>
    <div class="mt-0.5 truncate text-caption text-fg-muted">{subtitle}</div>
  </header>

  <div class="shrink-0 px-2 py-2">
    <Input
      type="search"
      placeholder={mode === "saved-queries"
        ? "Filter saved queries…"
        : "Filter history…"}
      bind:value={query}
      inputSize="sm"
    />
  </div>

  <div class="flex-1 overflow-auto px-1.5 pb-2">
    {#if count === 0}
      <p class="m-2 text-caption leading-relaxed text-fg-muted">
        {#if query.trim()}
          No matches for “{query.trim()}”.
        {:else if mode === "saved-queries"}
          No saved queries yet.
        {:else}
          No query history yet.
        {/if}
      </p>
    {:else if mode === "saved-queries"}
      {#each savedGroups as [folder, items] (folder)}
        <div class="mb-2">
          <div
            class="px-2 py-1 text-caption font-semibold tracking-wide text-fg-muted uppercase"
          >
            {folder}
          </div>
          <ul class="m-0 list-none p-0">
            {#each items as item (item.id)}
              {@const active = selectedId === item.id}
              <li>
                <button
                  type="button"
                  class={cn(
                    "flex w-full flex-col gap-0.5 rounded-md border-none px-2 py-1.5 text-left transition-colors",
                    active
                      ? "bg-accent-muted text-accent"
                      : "bg-transparent text-fg hover:bg-hover",
                  )}
                  onclick={() => onselect(item.id)}
                >
                  <span class="flex items-center gap-1.5">
                    <svg
                      class={cn(
                        "size-3.5 shrink-0",
                        active ? "text-accent" : "text-fg-muted",
                      )}
                      viewBox="0 0 16 16"
                      fill="none"
                      aria-hidden="true"
                    >
                      <path
                        d="M4 2.75h5.2L12 5.6v7.65a.75.75 0 0 1-.75.75h-7.5a.75.75 0 0 1-.75-.75V3.5A.75.75 0 0 1 4 2.75z"
                        stroke="currentColor"
                        stroke-width="1.25"
                      />
                      <path
                        d="M9 2.9v2.85h2.85"
                        stroke="currentColor"
                        stroke-width="1.25"
                        stroke-linejoin="round"
                      />
                    </svg>
                    <span class="min-w-0 flex-1 truncate text-label font-medium">
                      {item.name}
                    </span>
                  </span>
                  <span
                    class={cn(
                      "truncate pl-5 font-mono text-caption",
                      active ? "text-accent/80" : "text-fg-muted",
                    )}
                  >
                    {previewSql(item.sql, 40)}
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/each}
    {:else}
      <ul class="m-0 list-none p-0">
        {#each filteredHistory as item (item.id)}
          {@const active = selectedId === item.id}
          <li>
            <button
              type="button"
              class={cn(
                "mb-0.5 flex w-full flex-col gap-0.5 rounded-md border-none px-2 py-1.5 text-left transition-colors",
                active
                  ? "bg-accent-muted text-accent"
                  : "bg-transparent text-fg hover:bg-hover",
              )}
              onclick={() => onselect(item.id)}
            >
              <span class="flex items-center gap-1.5">
                <span
                  class={cn(
                    "size-1.5 shrink-0 rounded-full",
                    item.status === "success" ? "bg-success" : "bg-danger",
                  )}
                  title={item.status}
                  aria-hidden="true"
                ></span>
                <span
                  class={cn(
                    "min-w-0 flex-1 truncate font-mono text-label",
                    active ? "text-accent" : "text-fg",
                  )}
                >
                  {previewSql(item.sql, 36)}
                </span>
              </span>
              <span
                class={cn(
                  "flex items-center gap-1.5 pl-3.5 text-caption",
                  active ? "text-accent/80" : "text-fg-muted",
                )}
              >
                <span>{formatRelativeTime(item.ranAt)}</span>
                <span aria-hidden="true">·</span>
                <span class="font-mono tabular-nums">{item.durationMs}ms</span>
                {#if item.status === "success" && item.rowCount != null}
                  <span aria-hidden="true">·</span>
                  <span class="font-mono tabular-nums">{item.rowCount} rows</span>
                {/if}
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
    {count}
    {mode === "saved-queries" ? "saved" : "runs"}
  </footer>
</aside>
