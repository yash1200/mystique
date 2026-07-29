<script lang="ts">
  import { Button, Select } from "$lib/components/ui";

  interface Props {
    filteredCount: number;
    pageStart: number;
    pageEnd: number;
    filtersActive: boolean;
    hasActiveFilters: boolean;
    tableRowCount: number;
    dirtyCount: number;
    statusMsg: string | null;
    pageSize: number;
    pageSizeOptions: { value: string; label: string }[];
    safePage: number;
    totalPages: number;
    refreshing: boolean;
    saving: boolean;
    onSetPageSize: (v: string) => void;
    onPrev: () => void;
    onNext: () => void;
    onRefresh: () => void;
    onAddRow: () => void;
    onReset: () => void;
    onSave: () => void;
  }

  let {
    filteredCount,
    pageStart,
    pageEnd,
    filtersActive,
    hasActiveFilters,
    tableRowCount,
    dirtyCount,
    statusMsg,
    pageSize,
    pageSizeOptions,
    safePage,
    totalPages,
    refreshing,
    saving,
    onSetPageSize,
    onPrev,
    onNext,
    onRefresh,
    onAddRow,
    onReset,
    onSave,
  }: Props = $props();

  const hasChanges = $derived(dirtyCount > 0);
</script>

<!-- Match explorer / sessions rail: h-statusbar (34px), single row -->
<footer
  class="flex h-statusbar min-h-statusbar shrink-0 items-center gap-2 overflow-hidden border-t border-line bg-elevated px-2.5"
>
  <!-- Left: row counts -->
  <div
    class="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden text-caption text-fg-muted"
  >
    {#if filteredCount === 0}
      <span class="truncate font-mono tabular-nums">0 rows</span>
    {:else}
      <span class="truncate font-mono tabular-nums">
        {pageStart + 1}–{pageEnd} of {filteredCount}{#if filtersActive && hasActiveFilters}
          {" "}filtered{/if}
        · ~{tableRowCount.toLocaleString()} total
      </span>
    {/if}
    {#if hasChanges}
      <span
        class="shrink-0 rounded-sm border border-warning/45 bg-warning/15 px-1 py-px text-micro font-medium text-warning"
      >
        {dirtyCount} unsaved
      </span>
    {/if}
    {#if statusMsg}
      <span class="shrink-0 truncate text-success">{statusMsg}</span>
    {/if}
  </div>

  <!-- Center: pagination -->
  <div
    class="flex shrink-0 items-center gap-1.5"
    role="group"
    aria-label="Pagination"
  >
    <div class="w-24 shrink-0">
      <Select
        id="grid-page-size"
        value={String(pageSize)}
        options={pageSizeOptions}
        inputSize="sm"
        onchange={onSetPageSize}
      />
    </div>
    <div class="flex items-center gap-0.5">
      <Button
        variant="ghost"
        size="sm"
        disabled={safePage <= 0}
        onclick={onPrev}
        aria-label="Previous page"
      >
        ‹
      </Button>
      <span
        class="min-w-12 text-center font-mono text-caption text-fg-secondary tabular-nums"
      >
        {safePage + 1}/{totalPages}
      </span>
      <Button
        variant="ghost"
        size="sm"
        disabled={safePage >= totalPages - 1}
        onclick={onNext}
        aria-label="Next page"
      >
        ›
      </Button>
    </div>
  </div>

  <!-- Right: actions -->
  <div class="flex min-w-0 flex-1 items-center justify-end gap-1">
    <Button
      variant="ghost"
      size="sm"
      disabled={refreshing}
      onclick={onRefresh}
      title="Refresh (⌘/Ctrl+R)"
    >
      {refreshing ? "Refreshing…" : "Refresh"}
    </Button>

    <Button
      variant="ghost"
      size="sm"
      disabled={saving || refreshing}
      onclick={onAddRow}
      title="Add a new empty row"
    >
      Add row
    </Button>

    {#if hasChanges}
      <Button
        variant="ghost"
        size="sm"
        disabled={saving}
        onclick={onReset}
        title="Discard unsaved changes"
      >
        Reset
      </Button>

      <Button
        variant="primary"
        size="sm"
        disabled={saving}
        onclick={onSave}
        title="Save changes (⌘/Ctrl+S)"
      >
        {saving ? "Saving…" : "Save"}
      </Button>
    {/if}
  </div>
</footer>
