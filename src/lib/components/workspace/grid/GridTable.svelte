<script lang="ts">
  import type { ColumnReference, TableMeta } from "$lib/types/workspace";
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
    /** Unsaved inserted / cloned rows (not yet written). */
    isNewRow?: (row: number) => boolean;
    /** Rows marked for delete — still visible until Save. */
    isDeletedRow?: (row: number) => boolean;
    onSelectCell: (row: number, col: string) => void;
    onSelectRow: (row: number) => void;
    onStartEdit: (row: number, col: string, e?: MouseEvent) => void;
    onInputKeydown: (e: KeyboardEvent) => void;
    onCommitEdit: () => void;
    onRowMenu?: (row: number, e: MouseEvent) => void;
    /** Plugin-owned FK jump: open referenced table filtered to this value. */
    onOpenReference?: (ref: ColumnReference, value: unknown) => void;
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
    isNewRow = () => false,
    isDeletedRow = () => false,
    onSelectCell,
    onSelectRow,
    onStartEdit,
    onInputKeydown,
    onCommitEdit,
    onRowMenu,
    onOpenReference,
  }: Props = $props();
</script>

<div class="min-h-0 flex-1 overflow-auto" data-grid-scroll>
  <table class="w-full border-collapse text-left">
    <thead class="sticky top-0 z-10">
      <tr class="bg-elevated">
        <th
          class="w-10 min-w-10 select-none border-b border-line px-2 py-1.5 text-caption font-semibold text-fg-muted"
        >
          #
        </th>
        {#each table.columns as col (col.name)}
          <th
            class="max-w-64 min-w-32 border-b border-line px-2.5 py-1.5 text-caption font-semibold whitespace-nowrap text-fg-secondary"
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
        <!-- Absorbs leftover width so data columns keep a stable size. -->
        <th class="w-full min-w-0 border-b border-line p-0" aria-hidden="true"
        ></th>
      </tr>
    </thead>
    <tbody>
      {#each pageEntries as entry (entry.index)}
        {@const i = entry.index}
        {@const row = entry.row}
        {@const rowSelected = selectedRowIndex === i}
        {@const stripe = i % 2 === 1}
        {@const isNew = isNewRow(i)}
        {@const isDeleted = isDeletedRow(i)}
        <tr
          class={cn(
            "border-b border-line/70 transition-colors",
            isDeleted && "bg-danger/15",
            isDeleted && rowSelected && !selectedCell && "bg-danger/25",
            !isDeleted && isNew && "bg-info/12",
            !isDeleted && isNew && rowSelected && !selectedCell && "bg-info/22",
            !isDeleted &&
              !isNew &&
              rowSelected &&
              !selectedCell &&
              "bg-selected",
            !isDeleted && !isNew && !rowSelected && stripe && "bg-panel",
            !isDeleted &&
              !isNew &&
              !rowSelected &&
              !stripe &&
              "bg-transparent",
          )}
          aria-selected={rowSelected}
        >
          <td
            class={cn(
              "cursor-pointer select-none px-2 py-1 font-mono text-caption tabular-nums hover:bg-hover",
              isDeleted
                ? "font-medium text-danger"
                : isNew
                  ? "font-medium text-info"
                  : "text-fg-muted",
            )}
            title={isDeleted
              ? "Marked for delete — save to apply"
              : isNew
                ? "Unsaved new row"
                : undefined}
            onclick={(e) => {
              e.stopPropagation();
              onSelectRow(i);
            }}
            oncontextmenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onSelectRow(i);
              onRowMenu?.(i, e);
            }}
          >
            {#if isDeleted}
              <span class="sr-only">Marked for delete </span>
            {:else if isNew}
              <span class="sr-only">Unsaved new row </span>
            {/if}
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
                "relative max-w-64 min-w-32 border-l border-transparent p-0 font-mono text-label",
                isDeleted && "text-danger/80",
                dirty &&
                  !isDeleted &&
                  !isEditing &&
                  "bg-warning/15 ring-1 ring-inset ring-warning/40",
                isEditing && "bg-input ring-2 ring-inset ring-accent",
                isFocused &&
                  !isEditing &&
                  !isDeleted &&
                  "bg-selected ring-2 ring-inset ring-accent",
                !isFocused && !isEditing && !dirty && !isDeleted && "hover:bg-hover",
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
                  "flex min-h-grid-row items-center gap-1 px-2.5 py-1",
                  isEditing && "invisible",
                  isDeleted && "line-through opacity-80",
                  !isEditing && isNull && "italic text-fg-muted",
                  !isEditing && !isNull && !isDeleted && "text-fg",
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
                {#if !isNull && !isDeleted && col.ref && onOpenReference}
                  {@const ref = col.ref}
                  <button
                    type="button"
                    class="inline-grid size-5 shrink-0 place-items-center rounded-sm border border-transparent bg-transparent p-0 leading-none text-fg-muted transition-colors hover:border-accent/30 hover:bg-accent-muted hover:text-accent"
                    title={`Open ${ref.schema ? `${ref.schema}.` : ""}${ref.table}.${ref.column} = ${cellText(raw)}`}
                    aria-label={`Open referenced ${ref.table} row`}
                    onclick={(e) => {
                      e.stopPropagation();
                      e.preventDefault();
                      onOpenReference(ref, raw);
                    }}
                    ondblclick={(e) => {
                      e.stopPropagation();
                      e.preventDefault();
                    }}
                  >
                    <svg
                      class="pointer-events-none size-3.5 shrink-0"
                      viewBox="0 0 16 16"
                      fill="none"
                      aria-hidden="true"
                    >
                      <path
                        d="M6.5 3.5H4.25A1.75 1.75 0 0 0 2.5 5.25v6.5c0 .97.78 1.75 1.75 1.75h6.5c.97 0 1.75-.78 1.75-1.75V9.5"
                        stroke="currentColor"
                        stroke-width="1.25"
                        stroke-linecap="round"
                      />
                      <path
                        d="M9.5 2.5h4v4M13.5 2.5 8 8"
                        stroke="currentColor"
                        stroke-width="1.25"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      />
                    </svg>
                  </button>
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
          <td class="min-w-0 w-full p-0" aria-hidden="true"></td>
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
