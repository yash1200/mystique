<script lang="ts">
  import type { TableMeta } from "$lib/types/workspace";
  import { cn } from "$lib/utils/cn";
  import {
    selectOptionsForColumn,
    usesSelectEditor,
  } from "$lib/utils/columnEditor";
  import { cellText } from "./filterLogic";

  export type CellRef = { row: number; col: string };

  interface Props {
    table: TableMeta;
    pageEntries: { row: Record<string, unknown>; index: number }[];
    selectedRowIndex: number | null;
    /** Spreadsheet-style focused cell (absolute row index). */
    selectedCell: CellRef | null;
    originals: Record<string, unknown>[];
    editing: CellRef | null;
    draft?: string;
    inputEl?: HTMLInputElement | HTMLSelectElement | null;
    filtersActive: boolean;
    hasActiveFilters: boolean;
    isDirty: (row: number, col: string) => boolean;
    onSelectCell: (row: number, col: string) => void;
    onSelectRow: (row: number) => void;
    onStartEdit: (row: number, col: string, e?: MouseEvent) => void;
    onInputKeydown: (e: KeyboardEvent) => void;
    onCommitEdit: () => void;
  }

  let {
    table,
    pageEntries,
    selectedRowIndex,
    selectedCell,
    originals,
    editing,
    draft = $bindable(""),
    inputEl = $bindable<HTMLInputElement | HTMLSelectElement | null>(null),
    filtersActive,
    hasActiveFilters,
    isDirty,
    onSelectCell,
    onSelectRow,
    onStartEdit,
    onInputKeydown,
    onCommitEdit,
  }: Props = $props();
</script>

<div class="min-h-0 flex-1 overflow-auto" data-grid-scroll>
  <table class="w-full min-w-max border-collapse text-left">
    <thead class="sticky top-0 z-10">
      <tr class="bg-elevated">
        <th
          class="w-10 border-b border-line px-2 py-1.5 text-caption font-semibold text-fg-muted"
        >
          #
        </th>
        {#each table.columns as col (col.name)}
          <th
            class="border-b border-line px-2.5 py-1.5 text-caption font-semibold whitespace-nowrap text-fg-secondary"
            title={col.type}
          >
            <span class="text-fg">{col.name}</span>
            {#if col.pk}
              <span
                class="ml-1 text-caption font-semibold text-accent"
                title="Primary key">PK</span
              >
            {/if}
            <span class="ml-1.5 font-mono font-normal text-fg-muted"
              >{col.type}</span
            >
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each pageEntries as entry (entry.index)}
        {@const i = entry.index}
        {@const row = entry.row}
        {@const rowSelected = selectedRowIndex === i}
        {@const stripe = i % 2 === 1}
        <tr
          class={cn(
            "border-b border-line/70 transition-colors",
            rowSelected && !selectedCell && "bg-selected",
            !rowSelected && stripe && "bg-panel",
            !rowSelected && !stripe && "bg-transparent",
          )}
          aria-selected={rowSelected}
        >
          <td
            class="cursor-pointer px-2 py-1 font-mono text-caption text-fg-muted tabular-nums hover:bg-hover"
            onclick={(e) => {
              e.stopPropagation();
              onSelectRow(i);
            }}
          >
            {i + 1}
          </td>
          {#each table.columns as col (col.name)}
            {@const raw = row[col.name]}
            {@const isNull = raw === null || raw === undefined}
            {@const dirty = isDirty(i, col.name)}
            {@const isEditing =
              editing?.row === i && editing.col === col.name}
            {@const isFocused =
              selectedCell?.row === i && selectedCell.col === col.name}
            <td
              data-grid-cell
              data-row={i}
              data-col={col.name}
              class={cn(
                "relative max-w-64 cursor-cell border-l border-transparent p-0 font-mono text-label",
                dirty &&
                  !isEditing &&
                  "bg-warning/15 ring-1 ring-inset ring-warning/40",
                isEditing && "bg-input ring-2 ring-inset ring-accent",
                isFocused &&
                  !isEditing &&
                  "bg-selected ring-2 ring-inset ring-accent",
                !isFocused && !isEditing && !dirty && "hover:bg-hover",
              )}
              title={isEditing
                ? undefined
                : dirty
                  ? `Edited · original: ${cellText(originals[i]?.[col.name])}`
                  : cellText(raw)}
              onclick={(e) => {
                e.stopPropagation();
                onSelectCell(i, col.name);
              }}
              ondblclick={(e) => onStartEdit(i, col.name, e)}
            >
              <div
                class={cn(
                  "flex min-h-7 items-center gap-1 px-2.5 py-1",
                  isEditing && "invisible",
                  !isEditing && isNull && "italic text-fg-muted",
                  !isEditing && !isNull && "text-fg",
                )}
                aria-hidden={isEditing}
              >
                <span class="min-w-0 flex-1 truncate">{cellText(raw)}</span>
                {#if dirty}
                  <span
                    class="size-1.5 shrink-0 rounded-full bg-warning"
                    title="Edited"
                    aria-label="Edited"
                  ></span>
                {/if}
              </div>
              {#if isEditing}
                {@const selectMode = usesSelectEditor(col, raw)}
                {#if selectMode}
                  {@const options = selectOptionsForColumn(col, raw)}
                  <select
                    bind:this={inputEl}
                    class={cn(
                      "absolute inset-0 box-border h-full w-full border-none bg-input px-2 py-0.5",
                      "font-mono text-label text-fg focus:outline-none",
                    )}
                    bind:value={draft}
                    onkeydown={onInputKeydown}
                    onchange={onCommitEdit}
                    onblur={onCommitEdit}
                    onclick={(e) => e.stopPropagation()}
                    aria-label="Edit {col.name}"
                  >
                    {#each options as opt (opt.value === "" ? "__null__" : opt.value)}
                      <option value={opt.value}>{opt.label}</option>
                    {/each}
                  </select>
                {:else}
                  <input
                    bind:this={inputEl}
                    class={cn(
                      "absolute inset-0 box-border h-full w-full border-none bg-transparent px-2.5 py-1",
                      "font-mono text-label text-fg focus:outline-none",
                    )}
                    bind:value={draft}
                    onkeydown={onInputKeydown}
                    onblur={onCommitEdit}
                    onclick={(e) => e.stopPropagation()}
                    aria-label="Edit {col.name}"
                  />
                {/if}
              {/if}
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>

  {#if pageEntries.length === 0}
    <div class="px-4 py-8 text-center text-caption text-fg-muted">
      {#if filtersActive && hasActiveFilters}
        No rows match the current filters.
      {:else}
        No rows on this page.
      {/if}
    </div>
  {/if}
</div>
