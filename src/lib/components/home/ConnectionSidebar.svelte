<script lang="ts">
  import type { ConnectionProfile } from "$lib/types/connection";
  import {
    AppSettingsPopover,
    BrandMark,
    IconButton,
    Input,
  } from "$lib/components/ui";
  import ConnectionListItem from "./ConnectionListItem.svelte";

  interface Props {
    connections: ConnectionProfile[];
    selectedId: string | null;
    creating: boolean;
    onselect: (id: string) => void;
    onnew: () => void;
  }

  let {
    connections,
    selectedId,
    creating,
    onselect,
    onnew,
  }: Props = $props();

  let query = $state("");

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return connections;
    return connections.filter((c) => {
      const cfg = c.pluginConfig ?? {};
      const host = String(cfg.host ?? "").toLowerCase();
      const database = String(cfg.database ?? "").toLowerCase();
      const path = String(cfg.path ?? "").toLowerCase();
      return (
        c.name.toLowerCase().includes(q) ||
        host.includes(q) ||
        database.includes(q) ||
        path.includes(q) ||
        String(c.engine).toLowerCase().includes(q)
      );
    });
  });
</script>

<aside
  class="flex h-full w-sidebar min-w-60 max-w-panel-sm flex-col border-r border-line bg-panel"
  aria-label="Connections"
>
  <header class="px-4 pt-4 pb-3">
    <div class="flex items-center gap-3">
      <BrandMark />
      <div>
        <div class="text-body-lg font-semibold tracking-tight text-fg">
          Mystique<span class="font-mono text-label font-medium text-fg-muted"
            >DB</span
          >
        </div>
        <div class="text-caption text-fg-muted">Connections</div>
      </div>
    </div>
  </header>

  <div class="flex items-center gap-2 px-3 pb-3">
    <div class="min-w-0 flex-1">
      <span class="sr-only">Filter connections</span>
      <Input
        type="search"
        placeholder="Filter…"
        bind:value={query}
        inputSize="md"
      />
    </div>
    <IconButton
      label="New connection"
      variant="accent"
      size="md"
      active={creating}
      onclick={onnew}
    >
      +
    </IconButton>
  </div>

  <div
    class="flex flex-1 flex-col gap-0.5 overflow-auto px-2 pb-2"
    role="listbox"
    aria-label="Saved connections"
  >
    {#if filtered.length === 0}
      <p class="m-4 text-label leading-relaxed text-fg-muted">
        {#if connections.length === 0}
          No connections yet. Create one to get started.
        {:else}
          No matches for “{query}”.
        {/if}
      </p>
    {:else}
      {#each filtered as conn (conn.id)}
        <div role="option" aria-selected={selectedId === conn.id && !creating}>
          <ConnectionListItem
            connection={conn}
            selected={selectedId === conn.id && !creating}
            onclick={() => onselect(conn.id)}
          />
        </div>
      {/each}
    {/if}
  </div>

  <footer
    class="relative flex items-center justify-between border-t border-line px-3 py-2"
  >
    <span class="text-caption text-fg-muted">
      {connections.length} saved
    </span>
    <AppSettingsPopover placement="top-end" buttonSize="sm" />
  </footer>
</aside>
