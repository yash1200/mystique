<script lang="ts">
  import { onMount } from "svelte";
  import type { TableMeta } from "$lib/types/workspace";
  import GridFilterBar from "./grid/GridFilterBar.svelte";
  import GridTable, { type CellRef } from "./grid/GridTable.svelte";
  import GridStatusBar from "./grid/GridStatusBar.svelte";
  import {
    type FilterMode,
    type FilterOp,
    type FilterRule,
    opNeedsValue,
    newRule,
    matchRule,
    parseCodeFilters,
    rulesToCode,
    cellText,
    valuesEqual,
    parseDraft,
  } from "./grid/filterLogic";
  import {
    selectDraftToValue,
    usesSelectEditor,
    valueToSelectDraft,
  } from "$lib/utils/columnEditor";

  interface Props {
    table: TableMeta;
    selectedRowIndex: number | null;
    onselectrow: (index: number) => void;
    rowSyncTick?: number;
    onbaselinechange?: () => void;
  }

  let {
    table,
    selectedRowIndex,
    onselectrow,
    rowSyncTick = 0,
    onbaselinechange,
  }: Props = $props();

  let seed = $state<Record<string, unknown>[]>([]);
  let rows = $state<Record<string, unknown>[]>([]);
  let originals = $state<Record<string, unknown>[]>([]);
  let dirtyKeys = $state<Set<string>>(new Set());
  /** Absolute indices of rows added locally (not yet saved). */
  let newRowIndices = $state<Set<number>>(new Set());
  let editing = $state<CellRef | null>(null);
  /** Focused cell for keyboard nav / copy-paste (absolute row index). */
  let selectedCell = $state<CellRef | null>(null);
  let draft = $state("");
  let inputEl = $state<HTMLInputElement | HTMLSelectElement | null>(null);
  let gridShellEl = $state<HTMLDivElement | null>(null);

  let page = $state(0);
  let pageSize = $state(100);
  let refreshing = $state(false);
  let saving = $state(false);
  let statusMsg = $state<string | null>(null);

  let loadedTableId = $state<string | null>(null);
  let appliedSyncTick = $state(0);

  let filterMode = $state<FilterMode>("ui");
  let filterRules = $state<FilterRule[]>([]);
  let filterCode = $state("");
  let filtersActive = $state(false);

  const pageSizeOptions = [
    { value: "25", label: "25 / page" },
    { value: "50", label: "50 / page" },
    { value: "100", label: "100 / page" },
    { value: "200", label: "200 / page" },
  ];

  const columnOptions = $derived(
    table.columns.map((c) => ({ value: c.name, label: c.name })),
  );

  const tablePath = $derived(
    table.schema
      ? `${table.database}.${table.schema}.${table.name}`
      : `${table.database}.${table.name}`,
  );

  /** Unsaved cell edits + newly added rows (even if still empty). */
  const dirtyCount = $derived(dirtyKeys.size + newRowIndices.size);

  function makeRule(): FilterRule {
    return newRule(table.columns[0]?.name ?? "");
  }

  function activeRules(): FilterRule[] {
    if (!filtersActive) return [];
    if (filterMode === "code") {
      return parseCodeFilters(filterCode) ?? [];
    }
    return filterRules.filter((r) => {
      if (!r.column) return false;
      if (!opNeedsValue(r.op)) return true;
      return r.value.trim() !== "";
    });
  }

  const hasActiveFilters = $derived(activeRules().length > 0);

  const filteredEntries = $derived.by(() => {
    const rules = activeRules();
    if (rules.length === 0) {
      return rows.map((row, index) => ({ row, index }));
    }
    return rows
      .map((row, index) => ({ row, index }))
      .filter(({ row }) => rules.every((rule) => matchRule(row, rule)));
  });

  const filteredCount = $derived(filteredEntries.length);
  const totalPages = $derived(
    Math.max(1, Math.ceil(filteredCount / pageSize) || 1),
  );
  const safePage = $derived(Math.min(page, totalPages - 1));
  const pageStart = $derived(safePage * pageSize);
  const pageEnd = $derived(Math.min(pageStart + pageSize, filteredCount));
  const pageEntries = $derived(filteredEntries.slice(pageStart, pageEnd));

  function setFilterMode(mode: FilterMode) {
    if (mode === filterMode) return;
    if (mode === "code") {
      filterCode = rulesToCode(filterRules);
    } else {
      const parsed = parseCodeFilters(filterCode);
      if (parsed && parsed.length > 0) {
        filterRules = parsed;
      } else if (filterRules.length === 0) {
        filterRules = [makeRule()];
      }
    }
    filterMode = mode;
  }

  function applyFilters() {
    filtersActive = true;
    page = 0;
  }

  function clearFilters() {
    filtersActive = false;
    filterRules = [makeRule()];
    filterCode = "";
    page = 0;
  }

  function addRule() {
    filterRules = [...filterRules, makeRule()];
  }

  function removeRule(id: string) {
    filterRules = filterRules.filter((r) => r.id !== id);
    if (filterRules.length === 0) filterRules = [makeRule()];
  }

  function updateRule(id: string, patch: Partial<FilterRule>) {
    filterRules = filterRules.map((r) =>
      r.id === id ? { ...r, ...patch } : r,
    );
  }

  function updateFirstRule(patch: Partial<FilterRule>) {
    if (filterRules.length === 0) {
      filterRules = [{ ...makeRule(), ...patch }];
      return;
    }
    updateRule(filterRules[0].id, patch);
  }

  function cellKey(row: number, col: string): string {
    return `${row}::${col}`;
  }

  function isDirty(row: number, col: string): boolean {
    return dirtyKeys.has(cellKey(row, col));
  }

  function recomputeDirty(
    nextRows: Record<string, unknown>[],
    base: Record<string, unknown>[],
  ) {
    const next = new Set<string>();
    for (let i = 0; i < nextRows.length; i++) {
      for (const col of table.columns) {
        if (!valuesEqual(nextRows[i]?.[col.name], base[i]?.[col.name])) {
          next.add(cellKey(i, col.name));
        }
      }
    }
    dirtyKeys = next;
  }

  function loadFromTable(resetBaseline: boolean) {
    const cloned = table.rows.map((r) => ({ ...r }));
    rows = cloned.map((r) => ({ ...r }));
    if (resetBaseline) {
      seed = cloned.map((r) => ({ ...r }));
      originals = cloned.map((r) => ({ ...r }));
      dirtyKeys = new Set();
      newRowIndices = new Set();
    } else {
      recomputeDirty(cloned, originals);
    }
    editing = null;
    draft = "";
    loadedTableId = table.id;
  }

  function blankRow(): Record<string, unknown> {
    const row: Record<string, unknown> = {};
    for (const col of table.columns) {
      row[col.name] = null;
    }
    return row;
  }

  function addNewRow() {
    if (editing) commitEdit();
    const row = blankRow();
    const idx = rows.length;
    rows = [...rows, { ...row }];
    table.rows.push({ ...row });
    // Baseline equals blank so only real field edits add dirtyKeys;
    // membership in newRowIndices keeps Save/Reset visible.
    originals = [...originals, { ...row }];
    seed = [...seed, { ...row }];
    newRowIndices = new Set([...newRowIndices, idx]);
    // Jump to last page and focus first column
    page = Math.max(0, Math.ceil((idx + 1) / pageSize) - 1);
    const firstCol = table.columns[0]?.name;
    if (firstCol) {
      selectedCell = { row: idx, col: firstCol };
      onselectrow(idx);
      requestAnimationFrame(() => {
        scrollCellIntoView(idx, firstCol);
        focusGridShell();
      });
    }
    statusMsg = "Row added";
    setTimeout(() => {
      if (statusMsg === "Row added") statusMsg = null;
    }, 1500);
  }

  $effect(() => {
    const id = table.id;
    if (loadedTableId === id) return;
    loadFromTable(true);
    page = 0;
    statusMsg = null;
    filtersActive = false;
    filterRules = [makeRule()];
    filterCode = "";
    filterMode = "ui";
    appliedSyncTick = -1;
  });

  $effect(() => {
    const tick = rowSyncTick;
    const tableId = table.id;
    if (loadedTableId !== tableId) return;
    if (tick === appliedSyncTick) return;
    if (editing) return;
    appliedSyncTick = tick;
    loadFromTable(false);
  });

  $effect(() => {
    if (editing && inputEl) {
      inputEl.focus();
      if (inputEl instanceof HTMLInputElement) {
        inputEl.select();
      }
    }
  });

  $effect(() => {
    if (page > totalPages - 1) page = Math.max(0, totalPages - 1);
  });

  function columnDef(name: string) {
    return table.columns.find((c) => c.name === name);
  }

  function colNames(): string[] {
    return table.columns.map((c) => c.name);
  }

  function focusGridShell() {
    // Defer so we don't steal focus mid-edit open.
    queueMicrotask(() => {
      if (!editing) gridShellEl?.focus({ preventScroll: true });
    });
  }

  function selectCell(row: number, col: string) {
    if (editing) {
      if (editing.row === row && editing.col === col) return;
      commitEdit();
    }
    selectedCell = { row, col };
    onselectrow(row);
    focusGridShell();
    scrollCellIntoView(row, col);
  }

  function selectRowOnly(row: number) {
    if (editing) commitEdit();
    selectedCell = null;
    onselectrow(row);
    focusGridShell();
  }

  function scrollCellIntoView(row: number, col: string) {
    const root = gridShellEl;
    if (!root) return;
    const esc =
      typeof CSS !== "undefined" && typeof CSS.escape === "function"
        ? CSS.escape(col)
        : col.replace(/"/g, '\\"');
    const el = root.querySelector<HTMLElement>(
      `[data-grid-cell][data-row="${row}"][data-col="${esc}"]`,
    );
    el?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }

  /** Ensure absolute row is on the current page; change page if needed. */
  function ensureRowVisible(absoluteRow: number) {
    if (absoluteRow < pageStart) {
      page = Math.floor(absoluteRow / pageSize);
    } else if (absoluteRow >= pageEnd) {
      page = Math.floor(absoluteRow / pageSize);
    }
  }

  function moveSelection(dRow: number, dCol: number) {
    const cols = colNames();
    if (cols.length === 0 || filteredEntries.length === 0) return;

    let row = selectedCell?.row ?? filteredEntries[0]!.index;
    let colIdx = selectedCell
      ? Math.max(0, cols.indexOf(selectedCell.col))
      : 0;

    // Map absolute row → position in filtered list
    let filterIdx = filteredEntries.findIndex((e) => e.index === row);
    if (filterIdx < 0) filterIdx = 0;

    filterIdx = Math.max(
      0,
      Math.min(filteredEntries.length - 1, filterIdx + dRow),
    );
    colIdx = Math.max(0, Math.min(cols.length - 1, colIdx + dCol));

    const nextAbs = filteredEntries[filterIdx]!.index;
    const nextCol = cols[colIdx]!;
    ensureRowVisible(nextAbs);
    selectedCell = { row: nextAbs, col: nextCol };
    onselectrow(nextAbs);
    // Wait a frame for page change DOM
    requestAnimationFrame(() => scrollCellIntoView(nextAbs, nextCol));
  }

  function startEdit(
    row: number,
    col: string,
    e?: MouseEvent,
    /** When set (type-to-edit), replace cell with this seed text. */
    seedDraft?: string,
  ) {
    e?.stopPropagation();
    onselectrow(row);
    selectedCell = { row, col };
    if (editing?.row === row && editing.col === col && seedDraft === undefined)
      return;
    if (editing) commitEdit();
    const value = rows[row]?.[col];
    const def = columnDef(col);
    editing = { row, col };
    if (seedDraft !== undefined) {
      draft = seedDraft;
    } else if (def && usesSelectEditor(def, value)) {
      draft = valueToSelectDraft(value);
    } else {
      draft = value === null || value === undefined ? "" : cellText(value);
    }
  }

  function applyCellValue(row: number, col: string, rawText: string) {
    const prev = rows[row]?.[col];
    const def = columnDef(col);
    const next =
      def && usesSelectEditor(def, prev)
        ? selectDraftToValue(rawText, def, prev)
        : parseDraft(rawText, prev);
    rows = rows.map((r, i) => (i === row ? { ...r, [col]: next } : r));
    if (table.rows[row]) {
      table.rows[row] = { ...table.rows[row], [col]: next };
    } else {
      table.rows[row] = { ...(rows[row] ?? {}), [col]: next };
    }
    const key = cellKey(row, col);
    const original = originals[row]?.[col];
    const nextDirty = new Set(dirtyKeys);
    if (valuesEqual(next, original)) nextDirty.delete(key);
    else nextDirty.add(key);
    dirtyKeys = nextDirty;
  }

  function commitEdit() {
    if (!editing) return;
    const { row, col } = editing;
    applyCellValue(row, col, draft);
    selectedCell = { row, col };
    editing = null;
    draft = "";
    focusGridShell();
  }

  function cancelEdit() {
    if (!editing) return;
    selectedCell = { row: editing.row, col: editing.col };
    editing = null;
    draft = "";
    focusGridShell();
  }

  function onInputKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      commitEdit();
      // Excel-like: Enter moves down after commit
      moveSelection(1, 0);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      cancelEdit();
    } else if (e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      commitEdit();
      moveSelection(0, e.shiftKey ? -1 : 1);
    }
  }

  function copySelectedCell(e?: ClipboardEvent) {
    if (!selectedCell || editing) return;
    const v = rows[selectedCell.row]?.[selectedCell.col];
    // Clipboard: empty string for NULL (paste-friendly)
    const text = v === null || v === undefined ? "" : String(v);
    if (e?.clipboardData) {
      e.preventDefault();
      e.clipboardData.setData("text/plain", text);
    } else if (navigator.clipboard?.writeText) {
      void navigator.clipboard.writeText(text);
    }
    statusMsg = "Cell copied";
    setTimeout(() => {
      if (statusMsg === "Cell copied") statusMsg = null;
    }, 1200);
  }

  function pasteIntoGrid(text: string) {
    if (!selectedCell) return;
    const cols = colNames();
    if (cols.length === 0) return;

    const startColIdx = cols.indexOf(selectedCell.col);
    if (startColIdx < 0) return;

    // Normalize line endings; support TSV multi-cell paste from sheets
    const lines = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
    // Drop trailing empty line from Excel
    if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
    if (lines.length === 0) return;

    let filterIdx = filteredEntries.findIndex(
      (e) => e.index === selectedCell!.row,
    );
    if (filterIdx < 0) filterIdx = 0;

    let changed = 0;
    for (let r = 0; r < lines.length; r++) {
      const cells = lines[r]!.split("\t");
      const entry = filteredEntries[filterIdx + r];
      if (!entry) break;
      const absRow = entry.index;
      for (let c = 0; c < cells.length; c++) {
        const colName = cols[startColIdx + c];
        if (!colName) break;
        applyCellValue(absRow, colName, cells[c] ?? "");
        changed += 1;
      }
    }

    statusMsg =
      changed === 1 ? "Pasted 1 cell" : `Pasted ${changed} cells`;
    setTimeout(() => {
      if (statusMsg?.startsWith("Pasted")) statusMsg = null;
    }, 1500);
  }

  function onGridKeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;

    // ⌘/Ctrl+S — save (works even while a cell is being edited)
    if (mod && !e.altKey && (e.key === "s" || e.key === "S")) {
      e.preventDefault();
      e.stopPropagation();
      void saveChanges();
      return;
    }

    // Ignore when focus is in filter bar / other controls outside shell table
    const t = e.target as HTMLElement | null;
    if (
      t &&
      (t.tagName === "INPUT" ||
        t.tagName === "TEXTAREA" ||
        t.tagName === "SELECT" ||
        t.isContentEditable)
    ) {
      // Editing cell input has its own handler
      if (editing && (t === inputEl || inputEl?.contains(t))) return;
      // Filter inputs etc.
      if (!editing) return;
    }

    if (editing) return;

    if (mod && (e.key === "c" || e.key === "C")) {
      e.preventDefault();
      copySelectedCell();
      return;
    }
    // Ctrl/Cmd+V: let the `paste` event handle (has clipboardData).

    if (e.key === "Enter" && selectedCell) {
      e.preventDefault();
      startEdit(selectedCell.row, selectedCell.col);
      return;
    }

    if (e.key === "F2" && selectedCell) {
      e.preventDefault();
      startEdit(selectedCell.row, selectedCell.col);
      return;
    }

    if (e.key === "Escape") {
      e.preventDefault();
      selectedCell = null;
      return;
    }

    if (e.key === "ArrowUp") {
      e.preventDefault();
      moveSelection(-1, 0);
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      moveSelection(1, 0);
      return;
    }
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      moveSelection(0, -1);
      return;
    }
    if (e.key === "ArrowRight") {
      e.preventDefault();
      moveSelection(0, 1);
      return;
    }
    if (e.key === "Tab") {
      e.preventDefault();
      moveSelection(0, e.shiftKey ? -1 : 1);
      return;
    }

    // Type-to-edit: printable character replaces cell content
    if (
      selectedCell &&
      !mod &&
      !e.altKey &&
      e.key.length === 1 &&
      !e.metaKey
    ) {
      const def = columnDef(selectedCell.col);
      const prev = rows[selectedCell.row]?.[selectedCell.col];
      // Selects: open editor so user picks from list
      if (def && usesSelectEditor(def, prev)) {
        e.preventDefault();
        startEdit(selectedCell.row, selectedCell.col);
        return;
      }
      e.preventDefault();
      startEdit(selectedCell.row, selectedCell.col, undefined, e.key);
    }
  }

  function onGridCopy(e: ClipboardEvent) {
    if (editing) return;
    copySelectedCell(e);
  }

  function onGridPaste(e: ClipboardEvent) {
    if (editing) return;
    const text = e.clipboardData?.getData("text/plain");
    if (text == null) return;
    e.preventDefault();
    pasteIntoGrid(text);
  }

  // Clear cell focus when table reloads
  $effect(() => {
    void table.id;
    selectedCell = null;
  });

  function setPageSize(value: string) {
    pageSize = Number(value) || 100;
    page = 0;
  }

  function goPrev() {
    if (editing) commitEdit();
    page = Math.max(0, safePage - 1);
  }

  function goNext() {
    if (editing) commitEdit();
    page = Math.min(totalPages - 1, safePage + 1);
  }

  async function refresh() {
    if (refreshing) return;
    if (editing) cancelEdit();
    refreshing = true;
    statusMsg = null;
    await new Promise((r) => setTimeout(r, 350));
    const cloned = seed.map((r) => ({ ...r }));
    rows = cloned.map((r) => ({ ...r }));
    originals = cloned.map((r) => ({ ...r }));
    table.rows.splice(0, table.rows.length, ...cloned.map((r) => ({ ...r })));
    dirtyKeys = new Set();
    page = 0;
    refreshing = false;
    statusMsg = "Refreshed";
    onbaselinechange?.();
    setTimeout(() => {
      if (statusMsg === "Refreshed") statusMsg = null;
    }, 2000);
  }

  async function saveChanges() {
    if (saving) return;
    if (editing) commitEdit();
    if (dirtyCount === 0) return;
    saving = true;
    statusMsg = null;
    await new Promise((r) => setTimeout(r, 280));
    const snap = rows.map((r) => ({ ...r }));
    originals = snap.map((r) => ({ ...r }));
    seed = snap.map((r) => ({ ...r }));
    table.rows.splice(0, table.rows.length, ...snap.map((r) => ({ ...r })));
    dirtyKeys = new Set();
    newRowIndices = new Set();
    saving = false;
    statusMsg = "Changes saved";
    onbaselinechange?.();
    setTimeout(() => {
      if (statusMsg === "Changes saved") statusMsg = null;
    }, 2000);
  }

  // Global shortcuts while this table is open (filter bar, inspector, etc.)
  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
      const k = e.key.toLowerCase();
      if (k === "s") {
        e.preventDefault();
        e.stopPropagation();
        void saveChanges();
        return;
      }
      if (k === "r") {
        // Block browser reload; refresh table data instead.
        e.preventDefault();
        e.stopPropagation();
        void refresh();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  function resetChanges() {
    if (editing) cancelEdit();
    if (dirtyCount === 0) return;

    // Drop locally added rows, restore the rest from baseline
    if (newRowIndices.size > 0) {
      const keep = rows
        .map((r, i) => ({ r, i }))
        .filter(({ i }) => !newRowIndices.has(i));
      rows = keep.map(({ r }) => ({ ...r }));
      originals = keep.map(({ i }) => ({ ...(originals[i] ?? {}) }));
      seed = keep.map(({ i }) => ({ ...(seed[i] ?? {}) }));
      newRowIndices = new Set();
    }

    const restored = originals.map((r) => ({ ...r }));
    rows = restored.map((r) => ({ ...r }));
    table.rows.splice(0, table.rows.length, ...restored.map((r) => ({ ...r })));
    dirtyKeys = new Set();
    selectedCell = null;
    statusMsg = "Changes discarded";
    onbaselinechange?.();
    setTimeout(() => {
      if (statusMsg === "Changes discarded") statusMsg = null;
    }, 2000);
  }
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col">
  <GridFilterBar
    {filterMode}
    {filterRules}
    bind:filterCode
    {columnOptions}
    onSetMode={setFilterMode}
    onUpdateFirst={updateFirstRule}
    onUpdateRule={updateRule}
    onRemoveRule={removeRule}
    onAddRule={addRule}
    onApply={applyFilters}
    onClear={clearFilters}
  />

  <!-- tabindex so arrow/copy/paste work when grid is focused -->
  <div
    bind:this={gridShellEl}
    class="flex min-h-0 min-w-0 flex-1 flex-col outline-none"
    tabindex="0"
    role="grid"
    aria-label="Table data"
    onkeydown={onGridKeydown}
    oncopy={onGridCopy}
    onpaste={onGridPaste}
  >
    <GridTable
      {table}
      {pageEntries}
      {selectedRowIndex}
      {selectedCell}
      {originals}
      {editing}
      bind:draft
      bind:inputEl
      {filtersActive}
      {hasActiveFilters}
      {isDirty}
      onSelectCell={selectCell}
      onSelectRow={selectRowOnly}
      onStartEdit={startEdit}
      {onInputKeydown}
      onCommitEdit={commitEdit}
    />
  </div>

  <GridStatusBar
    {filteredCount}
    {pageStart}
    {pageEnd}
    {filtersActive}
    {hasActiveFilters}
    tableRowCount={Math.max(table.rowCount, rows.length)}
    {dirtyCount}
    {statusMsg}
    {pageSize}
    {pageSizeOptions}
    {safePage}
    {totalPages}
    {refreshing}
    {saving}
    onSetPageSize={setPageSize}
    onPrev={goPrev}
    onNext={goNext}
    onRefresh={() => void refresh()}
    onAddRow={addNewRow}
    onReset={resetChanges}
    onSave={() => void saveChanges()}
  />
</div>
