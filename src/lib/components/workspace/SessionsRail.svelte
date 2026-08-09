<script lang="ts">
  import type { ConnectedSession } from "$lib/stores/workspace-session.svelte";
  import { colorHex, engineLabel } from "$lib/types/connection";
  import {
    AppSettingsPopover,
    BrandMark,
    EngineBadge,
  } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";

  export type SideNavMode = "explorer" | "saved-queries" | "history";

  interface Props {
    sessions: ConnectedSession[];
    activeId: string | null;
    /** Which second-pane view is active. */
    mode?: SideNavMode;
    onselect: (id: string) => void;
    ondisconnect: (id: string) => void;
    onhome: () => void;
    onmode: (mode: SideNavMode) => void;
  }

  let {
    sessions,
    activeId,
    mode = "explorer",
    onselect,
    ondisconnect,
    onhome,
    onmode,
  }: Props = $props();

  function selectConnection(id: string) {
    onmode("explorer");
    onselect(id);
  }
</script>

<aside
  class="flex h-full w-12 shrink-0 flex-col items-center border-r border-line bg-panel"
  aria-label="Workspace navigation"
>
  <header
    class="flex h-topbar w-full shrink-0 items-center justify-center border-b border-line"
  >
    <button
      type="button"
      class="rounded-lg border-none bg-transparent p-0"
      onclick={onhome}
      title="Back to home"
      aria-label="Back to home"
    >
      <BrandMark size="sm" />
    </button>
  </header>

  <div
    class="flex w-full flex-1 flex-col items-center gap-1 overflow-y-auto overflow-x-hidden px-1.5 py-2"
  >
    <!-- Connected sessions -->
    <div
      class="flex w-full flex-col items-center gap-1"
      role="listbox"
      aria-label="Open connections"
    >
      {#if sessions.length === 0}
        <span
          class="mt-1 size-1.5 rounded-full bg-fg-muted/40"
          title="No live sessions"
          aria-label="No live sessions"
        ></span>
      {:else}
        {#each sessions as s (s.profile.id)}
          {@const active = mode === "explorer" && activeId === s.profile.id}
          {@const hex = colorHex(s.profile.colorId)}
          {@const prod = s.profile.environment === "production"}
          {@const label = `${s.profile.name} · ${engineLabel(s.profile.engine)}${prod ? " · PROD" : ""}`}
          <div
            class="group relative"
            role="option"
            aria-selected={active}
            style="--connection-color: {hex}"
          >
            <button
              type="button"
              class={cn(
                "relative grid size-9 place-items-center rounded-md border transition-colors",
                "focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1",
                active
                  ? "border-(--connection-color) bg-accent-muted ring-2 ring-(--connection-color)/30"
                  : prod
                    ? "border-warning/50 bg-warning/10 hover:bg-warning/15"
                    : "border-line bg-elevated hover:border-line-strong hover:bg-hover",
              )}
              title={label}
              aria-label={label}
              onclick={() => selectConnection(s.profile.id)}
            >
              <span
                class="absolute top-1 bottom-1 left-0 w-0.5 rounded-r-sm bg-conn"
                aria-hidden="true"
              ></span>
              <EngineBadge engine={s.profile.engine} size="sm" />
              <span
                class="absolute right-0.5 bottom-0.5 size-1.5 rounded-full bg-success ring-1 ring-elevated"
                title="Connected"
                aria-hidden="true"
              ></span>
            </button>

            <button
              type="button"
              class={cn(
                "absolute -top-1 -right-1 z-10 grid size-4 place-items-center rounded-full",
                "border border-line bg-elevated text-caption leading-none text-fg-muted shadow-sm",
                "opacity-0 transition-opacity hover:border-danger/50 hover:bg-danger/15 hover:text-danger",
                "group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100",
              )}
              title="Disconnect {s.profile.name}"
              aria-label="Disconnect {s.profile.name}"
              onclick={(e) => {
                e.stopPropagation();
                ondisconnect(s.profile.id);
              }}
            >
              ×
            </button>
          </div>
        {/each}
      {/if}
    </div>

    <!-- Saved queries + history, directly under connection icons -->
    <div
      class="mt-1 flex w-full flex-col items-center gap-1 border-t border-line pt-2"
      role="toolbar"
      aria-label="Queries"
    >
      <button
        type="button"
        class={cn(
          "grid size-9 place-items-center rounded-md border transition-colors",
          "focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1",
          mode === "saved-queries"
            ? "border-accent bg-accent-muted text-accent"
            : "border-line bg-elevated text-fg-muted hover:border-line-strong hover:bg-hover hover:text-fg",
        )}
        title="Saved queries"
        aria-label="Saved queries"
        aria-pressed={mode === "saved-queries"}
        onclick={() => onmode("saved-queries")}
      >
        <svg class="size-4" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <path
            d="M4.25 2.75h7.5v10.5L8 11.1l-3.75 2.15V2.75z"
            stroke="currentColor"
            stroke-width="1.3"
            stroke-linejoin="round"
          />
        </svg>
      </button>

      <button
        type="button"
        class={cn(
          "grid size-9 place-items-center rounded-md border transition-colors",
          "focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1",
          mode === "history"
            ? "border-accent bg-accent-muted text-accent"
            : "border-line bg-elevated text-fg-muted hover:border-line-strong hover:bg-hover hover:text-fg",
        )}
        title="Query history"
        aria-label="Query history"
        aria-pressed={mode === "history"}
        onclick={() => onmode("history")}
      >
        <svg class="size-4" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <circle
            cx="8"
            cy="8"
            r="5.25"
            stroke="currentColor"
            stroke-width="1.3"
          />
          <path
            d="M8 5.25V8l2 1.5"
            stroke="currentColor"
            stroke-width="1.3"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>
    </div>
  </div>

  <footer
    class="flex min-h-statusbar w-full shrink-0 flex-col items-center justify-center gap-1 border-t border-line bg-elevated py-1.5"
    title="{sessions.length} session{sessions.length === 1 ? '' : 's'}"
  >
    <AppSettingsPopover placement="right-end" buttonSize="sm" />
    <span class="font-mono text-caption tabular-nums text-fg-muted">
      {sessions.length}
    </span>
  </footer>
</aside>
