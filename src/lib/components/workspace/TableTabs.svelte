<script lang="ts">
  import type { WorkspaceTab } from "$lib/types/workspace";
  import { cn } from "$lib/utils/cn";

  interface Props {
    tabs: WorkspaceTab[];
    activeTabId: string | null;
    connectionColor?: string | null;
    production?: boolean;
    /** Only tabs for the active connection (parent filters). */
    onactivate: (id: string) => void;
    onclose: (id: string) => void;
    /** Open / focus Query canvas for this connection. */
    onopenquery?: () => void;
  }

  let {
    tabs,
    activeTabId,
    connectionColor = null,
    production = false,
    onactivate,
    onclose,
    onopenquery,
  }: Props = $props();
</script>

<div
  class={cn(
    "scrollbar-none flex h-control-sm shrink-0 items-stretch overflow-x-auto overscroll-x-contain border-b border-line bg-panel",
    production && "bg-warning/[0.06]",
  )}
  style={connectionColor ? `--connection-color: ${connectionColor}` : undefined}
  role="tablist"
  aria-label="Workspace tabs"
  onwheel={(e) => {
    const el = e.currentTarget;
    if (el.scrollWidth <= el.clientWidth) return;
    if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;
    el.scrollLeft += e.deltaY;
    e.preventDefault();
  }}
>
  {#each tabs as tab (tab.id)}
    {@const active = activeTabId === tab.id}
    {@const dirty = !!tab.dirty}
    <div
      class={cn(
        "group/tab flex max-w-56 shrink-0 items-stretch border-r border-line",
        "relative",
        active
          ? "bg-app-wash text-fg"
          : "bg-transparent text-fg-secondary hover:bg-hover hover:text-fg",
      )}
      role="presentation"
    >
      {#if connectionColor}
        <span
          class="absolute inset-x-0 top-0 h-0.5 bg-conn"
          class:opacity-40={!active}
          aria-hidden="true"
        ></span>
      {/if}
      <button
        type="button"
        role="tab"
        aria-selected={active}
        class="min-w-0 flex-1 truncate border-none bg-transparent px-2.5 text-left text-label font-medium leading-none"
        title={dirty ? `${tab.title} (unsaved changes)` : tab.title}
        onclick={() => onactivate(tab.id)}
      >
        {#if tab.kind === "query"}
          <span class="text-accent">◇</span>
          {" "}
        {:else if tab.kind === "model"}
          <span class="text-info">⬡</span>
          {" "}
        {:else if tab.kind === "structure"}
          <span class="text-fg-muted">▣</span>
          {" "}
        {/if}
        {tab.title}
      </button>
      <button
        type="button"
        class={cn(
          "group/close mr-1 flex size-6 shrink-0 self-center items-center justify-center rounded-sm border-none bg-transparent text-body-lg font-medium",
          "hover:bg-hover hover:text-fg",
          // Clean inactive tabs: show close only when the tab row is hovered
          !dirty && !active && "opacity-0 group-hover/tab:opacity-100",
          dirty ? "text-warning" : "text-fg-muted",
        )}
        aria-label={dirty ? `Unsaved — close ${tab.title}` : `Close ${tab.title}`}
        title={dirty ? "Unsaved changes — click to close" : "Close"}
        onclick={(e) => {
          e.stopPropagation();
          onclose(tab.id);
        }}
      >
        <!-- Dirty: yellow dot; hover this control to reveal × -->
        {#if dirty}
          <span
            class="text-label leading-none group-hover/close:hidden"
            aria-hidden="true">●</span
          >
          <span
            class="hidden leading-none group-hover/close:inline"
            aria-hidden="true">×</span
          >
        {:else}
          <span class="leading-none" aria-hidden="true">×</span>
        {/if}
      </button>
    </div>
  {/each}

  {#if onopenquery}
    <button
      type="button"
      class="flex shrink-0 items-center border-none bg-transparent px-2 text-caption text-fg-muted hover:bg-hover hover:text-fg"
      title="New query tab (⌘/Ctrl+T)"
      aria-label="New query tab"
      onclick={onopenquery}
    >
      + Query
    </button>
  {/if}
</div>
