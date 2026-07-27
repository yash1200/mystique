<script lang="ts">
  import type { WorkspaceTab } from "$lib/types/workspace";
  import { cn } from "$lib/utils/cn";

  interface Props {
    tabs: WorkspaceTab[];
    activeTabId: string | null;
    /** Only tabs for the active connection (parent filters). */
    onactivate: (id: string) => void;
    onclose: (id: string) => void;
    /** Open / focus Query canvas for this connection. */
    onopenquery?: () => void;
  }

  let { tabs, activeTabId, onactivate, onclose, onopenquery }: Props = $props();
</script>

<div
  class="flex h-control-sm shrink-0 items-stretch overflow-x-auto border-b border-line bg-panel"
  role="tablist"
  aria-label="Workspace tabs"
>
  {#each tabs as tab (tab.id)}
    {@const active = activeTabId === tab.id}
    <div
      class={cn(
        "group flex max-w-52 shrink-0 items-stretch border-r border-line",
        active
          ? "bg-app-wash text-fg"
          : "bg-transparent text-fg-secondary hover:bg-hover hover:text-fg",
      )}
      role="presentation"
    >
      <button
        type="button"
        role="tab"
        aria-selected={active}
        class="min-w-0 flex-1 truncate border-none bg-transparent px-2.5 text-left text-label font-medium leading-none"
        onclick={() => onactivate(tab.id)}
      >
        {#if tab.kind === "query"}
          <span class="text-accent">◇</span>
          {" "}
        {/if}
        {tab.title}
      </button>
      <button
        type="button"
        class={cn(
          "mr-0.5 flex size-5 shrink-0 self-center items-center justify-center rounded-sm border-none bg-transparent text-caption text-fg-muted",
          "hover:bg-hover hover:text-fg",
          !active && "opacity-0 group-hover:opacity-100",
        )}
        aria-label="Close {tab.title}"
        onclick={(e) => {
          e.stopPropagation();
          onclose(tab.id);
        }}
      >
        ×
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
