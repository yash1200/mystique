<script lang="ts">
  import type { ConnectionProfile } from "$lib/types/connection";
  import { IconButton, Input, ThemeToggle } from "$lib/components/ui";
  import ConnectionListItem from "./ConnectionListItem.svelte";

  type ThemePref = "light" | "dark" | "system";

  interface Props {
    connections: ConnectionProfile[];
    selectedId: string | null;
    creating: boolean;
    onselect: (id: string) => void;
    onnew: () => void;
    themePreference?: ThemePref;
    onthemechange?: (pref: ThemePref) => void;
  }

  let {
    connections,
    selectedId,
    creating,
    onselect,
    onnew,
    themePreference = "dark",
    onthemechange,
  }: Props = $props();

  let query = $state("");
  let settingsOpen = $state(false);
  let settingsWrap: HTMLDivElement | undefined = $state();

  function toggleSettings() {
    settingsOpen = !settingsOpen;
  }

  function onWindowPointerDown(e: PointerEvent) {
    if (!settingsOpen) return;
    const t = e.target;
    if (t instanceof Node && settingsWrap?.contains(t)) return;
    settingsOpen = false;
  }

  function onWindowKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && settingsOpen) {
      settingsOpen = false;
    }
  }

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
      <span
        class="grid size-8 place-items-center rounded-lg bg-linear-to-br from-accent to-conn-indigo text-label font-bold text-fg-inverse"
        aria-hidden="true"
      >
        M
      </span>
      <div>
        <div class="text-body-lg font-semibold tracking-tight text-fg">
          MystiqueDB
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
    <div bind:this={settingsWrap} class="relative">
      <IconButton
        label="Settings"
        variant="ghost"
        size="sm"
        active={settingsOpen}
        onclick={toggleSettings}
        aria-expanded={settingsOpen}
        aria-haspopup="dialog"
      >
        <svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <path
            d="M6.6 1.85h2.8l.3 1.45c.42.16.81.38 1.16.66l1.48-.38 1.4 2.42-1.18.9c.07.3.11.62.11 1.1 0 .37-.04.69-.11 1.01l1.18.9-1.4 2.42-1.48-.38a4.6 4.6 0 0 1-1.16.66l-.3 1.45H6.6l-.3-1.45a4.6 4.6 0 0 1-1.16-.66l-1.48.38-1.4-2.42 1.18-.9A4.4 4.4 0 0 1 3.33 8c0-.37.04-.69.11-1.01l-1.18-.9 1.4-2.42 1.48.38c.35-.28.74-.5 1.16-.66l.3-1.45Z"
            stroke="currentColor"
            stroke-width="1.2"
            stroke-linejoin="round"
          />
          <circle
            cx="8"
            cy="8"
            r="1.65"
            stroke="currentColor"
            stroke-width="1.2"
          />
        </svg>
      </IconButton>
      {#if settingsOpen}
        <div
          class="absolute right-0 bottom-full z-20 mb-2 w-56 rounded-lg border border-line bg-elevated p-3 shadow-md"
          role="dialog"
          aria-label="App settings"
        >
          <p class="m-0 mb-2 text-label font-semibold tracking-wider text-fg-muted uppercase">
            Theme
          </p>
          <ThemeToggle preference={themePreference} onchange={onthemechange} />
        </div>
      {/if}
    </div>
  </footer>
</aside>

<svelte:window
  onpointerdown={onWindowPointerDown}
  onkeydown={onWindowKeydown}
/>
