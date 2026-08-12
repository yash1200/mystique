<script lang="ts">
  import { onMount } from "svelte";
  import { Input, StatusBar } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";
  import {
    formatRelativeTime,
    previewSql,
    type QueryHistoryEntry,
    type SavedQuery,
  } from "$lib/data/mock-queries";

  type Mode = "saved-queries" | "history";
  type SavedMenuAction = "open" | "duplicate" | "rename" | "delete";

  interface Props {
    mode: Mode;
    savedQueries: SavedQuery[];
    history: QueryHistoryEntry[];
    selectedId: string | null;
    onselect: (id: string) => void;
    /** Context menu actions for saved queries (open / duplicate / rename / delete). */
    onsavedaction?: (action: SavedMenuAction, id: string) => void;
  }

  let {
    mode,
    savedQueries,
    history,
    selectedId,
    onselect,
    onsavedaction,
  }: Props = $props();

  let query = $state("");
  let menuOpen = $state(false);
  let menuQueryId = $state<string | null>(null);
  let menuX = $state(0);
  let menuY = $state(0);
  let menuRoot = $state<HTMLDivElement | null>(null);

  const title = $derived(
    mode === "saved-queries" ? "Saved queries" : "Query history",
  );
  const subtitle = $derived(
    mode === "saved-queries"
      ? "Click to open · right-click for more"
      : "Click to open in the editor",
  );

  const filteredSaved = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return savedQueries;
    return savedQueries.filter(
      (s) =>
        s.name.toLowerCase().includes(q) || s.sql.toLowerCase().includes(q),
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

  const count = $derived(
    mode === "saved-queries" ? filteredSaved.length : filteredHistory.length,
  );

  const menuQuery = $derived(
    menuQueryId
      ? (savedQueries.find((s) => s.id === menuQueryId) ?? null)
      : null,
  );

  function openMenu(e: MouseEvent, id: string) {
    e.preventDefault();
    e.stopPropagation();
    menuQueryId = id;
    // Keep menu on-screen
    const pad = 8;
    const mw = 160;
    const mh = 160;
    menuX = Math.min(e.clientX, window.innerWidth - mw - pad);
    menuY = Math.min(e.clientY, window.innerHeight - mh - pad);
    menuOpen = true;
  }

  function closeMenu() {
    menuOpen = false;
    menuQueryId = null;
  }

  function pick(action: SavedMenuAction) {
    const id = menuQueryId;
    closeMenu();
    if (!id) return;
    if (action === "open") {
      onselect(id);
      return;
    }
    onsavedaction?.(action, id);
  }

  onMount(() => {
    const onDoc = (e: PointerEvent) => {
      if (!menuOpen || !menuRoot) return;
      if (e.target instanceof Node && menuRoot.contains(e.target)) return;
      closeMenu();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeMenu();
    };
    const onScroll = () => {
      if (menuOpen) closeMenu();
    };
    document.addEventListener("pointerdown", onDoc, true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("pointerdown", onDoc, true);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", onScroll, true);
    };
  });
</script>

<aside
  class="flex h-full w-full min-w-0 flex-col bg-panel"
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
          No saved queries yet. Save a query from the editor.
        {:else}
          No query history yet.
        {/if}
      </p>
    {:else if mode === "saved-queries"}
      <ul class="m-0 list-none p-0">
        {#each filteredSaved as item (item.id)}
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
              oncontextmenu={(e) => openMenu(e, item.id)}
            >
              <span class="truncate text-label font-medium">{item.name}</span>
              <span
                class={cn(
                  "text-caption",
                  active ? "text-accent/70" : "text-fg-muted",
                )}
              >
                {formatRelativeTime(item.updatedAt)}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <ul class="m-0 list-none p-0">
        {#each filteredHistory as item (item.id)}
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
              <span
                class={cn(
                  "line-clamp-2 font-mono text-label",
                  active ? "text-accent" : "text-fg",
                )}
              >
                {previewSql(item.sql)}
              </span>
              <span
                class={cn(
                  "flex flex-wrap items-center gap-1.5 text-caption",
                  active ? "text-accent/70" : "text-fg-muted",
                )}
              >
                <span
                  class={item.status === "success"
                    ? "text-success"
                    : "text-danger"}
                >
                  {item.status === "success" ? "ok" : "err"}
                </span>
                <span>·</span>
                <span>{formatRelativeTime(item.ranAt)}</span>
                {#if item.durationMs != null}
                  <span>·</span>
                  <span>{item.durationMs}ms</span>
                {/if}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <StatusBar ariaLabel="Query list status">
    {#snippet left()}
      <span class="tabular-nums">{count}</span>
    {/snippet}
  </StatusBar>
</aside>

{#if menuOpen && menuQuery}
  <div
    bind:this={menuRoot}
    class="fixed z-50 min-w-40 rounded-md border border-line bg-elevated py-1 shadow-md"
    style="left: {menuX}px; top: {menuY}px"
    role="menu"
    aria-label="Saved query actions"
  >
    <button
      type="button"
      role="menuitem"
      class="flex w-full border-none bg-transparent px-2.5 py-1.5 text-left text-label text-fg hover:bg-hover"
      onclick={() => pick("open")}
    >
      Open
    </button>
    <button
      type="button"
      role="menuitem"
      class="flex w-full border-none bg-transparent px-2.5 py-1.5 text-left text-label text-fg hover:bg-hover"
      onclick={() => pick("duplicate")}
    >
      Duplicate
    </button>
    <button
      type="button"
      role="menuitem"
      class="flex w-full border-none bg-transparent px-2.5 py-1.5 text-left text-label text-fg hover:bg-hover"
      onclick={() => pick("rename")}
    >
      Rename
    </button>
    <div class="my-1 border-t border-line"></div>
    <button
      type="button"
      role="menuitem"
      class="flex w-full border-none bg-transparent px-2.5 py-1.5 text-left text-label text-danger hover:bg-danger/10"
      onclick={() => pick("delete")}
    >
      Delete
    </button>
  </div>
{/if}
