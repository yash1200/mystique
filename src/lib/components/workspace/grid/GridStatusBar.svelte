<script lang="ts">
  import { Button, Pager, StatusBar } from "$lib/components/ui";

  interface Props {
    filteredCount: number;
    pageStart: number;
    pageEnd: number;
    filtersActive: boolean;
    hasActiveFilters: boolean;
    tableRowCount: number;
    dirtyCount: number;
    statusMsg: string | null;
    safePage: number;
    totalPages: number;
    refreshing: boolean;
    saving: boolean;
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
    safePage,
    totalPages,
    refreshing,
    saving,
    onPrev,
    onNext,
    onRefresh,
    onAddRow,
    onReset,
    onSave,
  }: Props = $props();

  const hasChanges = $derived(dirtyCount > 0);
</script>

<StatusBar ariaLabel="Table status">
  {#snippet left()}
    {#if filteredCount === 0}
      <span class="truncate font-mono tabular-nums">0 rows</span>
    {:else}
      <span class="truncate font-mono tabular-nums">
        {pageStart + 1}–{pageEnd} of
        {#if filtersActive && hasActiveFilters}
          {filteredCount} filtered · {tableRowCount.toLocaleString()} total
        {:else}
          {tableRowCount.toLocaleString()}
        {/if}
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
  {/snippet}
  {#snippet center()}
    <Pager
      page={safePage}
      {totalPages}
      disabled={refreshing || saving}
      onprev={onPrev}
      onnext={onNext}
    />
  {/snippet}
  {#snippet right()}
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
      title="Add a new empty row (⌘/Ctrl+N)"
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
  {/snippet}
</StatusBar>
