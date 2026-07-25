<script lang="ts">
  import type { TableMeta } from "$lib/types/workspace";
  import { Button, IconButton, Input, Select } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";

  interface Props {
    table: TableMeta;
    selectedRowIndex: number | null;
    onselectrow: (index: number) => void;
    /**
     * Bumps when another UI (row inspector) mutates `table.rows`.
     * Re-syncs the grid without resetting the save baseline.
     */
    rowSyncTick?: number;
    /** Fires when the save baseline changes (save / reset / refresh only). */
    onbaselinechange?: () => void;
  }

  type FilterMode = "ui" | "code";
  type FilterOp =
    | "eq"
    | "neq"
    | "gt"
    | "gte"
    | "lt"
    | "lte"
    | "contains"
    | "starts"
    | "ends"
    | "is_null"
    | "not_null"
    | "is_empty"
    | "not_empty";

  type FilterRule = {
    id: string;
    column: string;
    op: FilterOp;
    value: string;
  };

  let {
    table,
    selectedRowIndex,
    onselectrow,
    rowSyncTick = 0,
    onbaselinechange,
  }: Props = $props();

  /** Last saved / server snapshot (refresh restores this). */
  let seed = $state<Record<string, unknown>[]>([]);
  /** Working copy (edits live here + mirrored onto `table.rows`). */
  let rows = $state<Record<string, unknown>[]>([]);
  /** Baseline for dirty detection (updated on save). */
  let originals = $state<Record<string, unknown>[]>([]);
  /** Keys: `${rowIndex}::${columnName}` */
  let dirtyKeys = $state<Set<string>>(new Set());
  let editing = $state<{ row: number; col: string } | null>(null);
  let draft = $state("");
  let inputEl = $state<HTMLInputElement | null>(null);

  let page = $state(0);
  let pageSize = $state(50);
  let refreshing = $state(false);
  let saving = $state(false);
  let statusMsg = $state<string | null>(null);

  /** Tracks which table the local buffers belong to. */
  let loadedTableId = $state<string | null>(null);
  /** Last applied external sync tick. */
  let appliedSyncTick = $state(0);

  /** Filter bar: visual builder vs raw expression. */
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

  const FILTER_OPS: { value: FilterOp; label: string; needsValue: boolean }[] =
    [
      { value: "eq", label: "= equals", needsValue: true },
      { value: "neq", label: "≠ not equal", needsValue: true },
      { value: "gt", label: "> greater", needsValue: true },
      { value: "gte", label: "≥ greater or equal", needsValue: true },
      { value: "lt", label: "< less", needsValue: true },
      { value: "lte", label: "≤ less or equal", needsValue: true },
      { value: "contains", label: "contains", needsValue: true },
      { value: "starts", label: "starts with", needsValue: true },
      { value: "ends", label: "ends with", needsValue: true },
      { value: "is_null", label: "is null", needsValue: false },
      { value: "not_null", label: "is not null", needsValue: false },
      { value: "is_empty", label: "is empty", needsValue: false },
      { value: "not_empty", label: "is not empty", needsValue: false },
    ];

  const opOptions = FILTER_OPS.map((o) => ({
    value: o.value,
    label: o.label,
  }));

  const columnOptions = $derived(
    table.columns.map((c) => ({
      value: c.name,
      label: c.name,
    })),
  );

  const tablePath = $derived(
    table.schema
      ? `${table.database}.${table.schema}.${table.name}`
      : `${table.database}.${table.name}`,
  );

  const dirtyCount = $derived(dirtyKeys.size);

  function opNeedsValue(op: FilterOp): boolean {
    return FILTER_OPS.find((o) => o.value === op)?.needsValue ?? true;
  }

  function newRule(): FilterRule {
    return {
      id: `f-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
      column: table.columns[0]?.name ?? "",
      op: "eq",
      value: "",
    };
  }

  function matchRule(row: Record<string, unknown>, rule: FilterRule): boolean {
    if (!rule.column) return true;
    const raw = row[rule.column];
    const isNull = raw === null || raw === undefined;
    const str = isNull ? "" : String(raw);
    const needle = rule.value;

    switch (rule.op) {
      case "is_null":
        return isNull;
      case "not_null":
        return !isNull;
      case "is_empty":
        return !isNull && str.trim() === "";
      case "not_empty":
        return !isNull && str.trim() !== "";
      case "contains":
        return str.toLowerCase().includes(needle.toLowerCase());
      case "starts":
        return str.toLowerCase().startsWith(needle.toLowerCase());
      case "ends":
        return str.toLowerCase().endsWith(needle.toLowerCase());
      case "eq":
        if (needle.trim().toUpperCase() === "NULL") return isNull;
        if (typeof raw === "number" && needle.trim() !== "" && !Number.isNaN(Number(needle))) {
          return raw === Number(needle);
        }
        if (typeof raw === "boolean") {
          const n = needle.trim().toLowerCase();
          return (
            (raw && (n === "true" || n === "1")) ||
            (!raw && (n === "false" || n === "0"))
          );
        }
        return str === needle;
      case "neq":
        return !matchRule(row, { ...rule, op: "eq" });
      case "gt":
      case "gte":
      case "lt":
      case "lte": {
        const a = typeof raw === "number" ? raw : Number(str);
        const b = Number(needle);
        if (Number.isNaN(a) || Number.isNaN(b)) {
          // fallback string compare
          if (rule.op === "gt") return str > needle;
          if (rule.op === "gte") return str >= needle;
          if (rule.op === "lt") return str < needle;
          return str <= needle;
        }
        if (rule.op === "gt") return a > b;
        if (rule.op === "gte") return a >= b;
        if (rule.op === "lt") return a < b;
        return a <= b;
      }
      default:
        return true;
    }
  }

  /** Parse simple code filters: `col = value` lines joined by AND. */
  function parseCodeFilters(code: string): FilterRule[] | null {
    const lines = code
      .split(/\n+|(?:\s+AND\s+)/i)
      .map((l) => l.trim())
      .filter(Boolean);
    if (lines.length === 0) return [];
    const rules: FilterRule[] = [];
    for (const line of lines) {
      const m = line.match(
        /^([A-Za-z_][\w.]*)\s*(=|!=|<>|>=|<=|>|<|contains|starts with|ends with|is null|is not null|is empty|is not empty)\s*(.*)$/i,
      );
      if (!m) return null;
      const col = m[1];
      const opRaw = m[2].toLowerCase();
      let val = m[3].trim();
      if (
        (val.startsWith("'") && val.endsWith("'")) ||
        (val.startsWith('"') && val.endsWith('"'))
      ) {
        val = val.slice(1, -1);
      }
      let op: FilterOp = "eq";
      if (opRaw === "=") op = "eq";
      else if (opRaw === "!=" || opRaw === "<>") op = "neq";
      else if (opRaw === ">") op = "gt";
      else if (opRaw === ">=") op = "gte";
      else if (opRaw === "<") op = "lt";
      else if (opRaw === "<=") op = "lte";
      else if (opRaw === "contains") op = "contains";
      else if (opRaw === "starts with") op = "starts";
      else if (opRaw === "ends with") op = "ends";
      else if (opRaw === "is null") op = "is_null";
      else if (opRaw === "is not null") op = "not_null";
      else if (opRaw === "is empty") op = "is_empty";
      else if (opRaw === "is not empty") op = "not_empty";
      rules.push({
        id: `f-${rules.length}-${col}`,
        column: col,
        op,
        value: val,
      });
    }
    return rules;
  }

  function rulesToCode(rules: FilterRule[]): string {
    return rules
      .filter((r) => r.column)
      .map((r) => {
        const col = r.column;
        switch (r.op) {
          case "eq":
            return `${col} = '${r.value}'`;
          case "neq":
            return `${col} != '${r.value}'`;
          case "gt":
            return `${col} > ${r.value}`;
          case "gte":
            return `${col} >= ${r.value}`;
          case "lt":
            return `${col} < ${r.value}`;
          case "lte":
            return `${col} <= ${r.value}`;
          case "contains":
            return `${col} contains '${r.value}'`;
          case "starts":
            return `${col} starts with '${r.value}'`;
          case "ends":
            return `${col} ends with '${r.value}'`;
          case "is_null":
            return `${col} is null`;
          case "not_null":
            return `${col} is not null`;
          case "is_empty":
            return `${col} is empty`;
          case "not_empty":
            return `${col} is not empty`;
        }
      })
      .join("\nAND ");
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
        filterRules = [newRule()];
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
    filterRules = [newRule()];
    filterCode = "";
    page = 0;
  }

  function addRule() {
    filterRules = [...filterRules, newRule()];
  }

  function removeRule(id: string) {
    filterRules = filterRules.filter((r) => r.id !== id);
    if (filterRules.length === 0) filterRules = [newRule()];
  }

  function updateRule(id: string, patch: Partial<FilterRule>) {
    filterRules = filterRules.map((r) =>
      r.id === id ? { ...r, ...patch } : r,
    );
  }

  function updateFirstRule(patch: Partial<FilterRule>) {
    if (filterRules.length === 0) {
      filterRules = [{ ...newRule(), ...patch }];
      return;
    }
    const id = filterRules[0].id;
    updateRule(id, patch);
  }

  function cellKey(row: number, col: string): string {
    return `${row}::${col}`;
  }

  function isDirty(row: number, col: string): boolean {
    return dirtyKeys.has(cellKey(row, col));
  }

  function valuesEqual(a: unknown, b: unknown): boolean {
    if (a === b) return true;
    if (a == null && b == null) return true;
    return String(a) === String(b);
  }

  function cellText(value: unknown): string {
    if (value === null || value === undefined) return "NULL";
    if (typeof value === "boolean") return value ? "true" : "false";
    return String(value);
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
    } else {
      recomputeDirty(cloned, originals);
    }
    editing = null;
    draft = "";
    loadedTableId = table.id;
  }

  // Load / switch table — do NOT call parent setters here (avoids update loops).
  $effect(() => {
    const id = table.id;
    if (loadedTableId === id) return;
    loadFromTable(true);
    page = 0;
    statusMsg = null;
    filtersActive = false;
    filterRules = [newRule()];
    filterCode = "";
    filterMode = "ui";
    // Align sync cursor without tracking rowSyncTick as a dependency of this effect.
    appliedSyncTick = -1;
  });

  // External edits from row inspector.
  $effect(() => {
    const tick = rowSyncTick;
    const tableId = table.id;
    if (loadedTableId !== tableId) return;
    if (tick === appliedSyncTick) return;
    if (editing) return;
    appliedSyncTick = tick;
    // tick 0 on fresh mount: still safe — just re-reads same data once
    loadFromTable(false);
  });

  $effect(() => {
    if (editing && inputEl) {
      inputEl.focus();
      inputEl.select();
    }
  });

  $effect(() => {
    if (page > totalPages - 1) page = Math.max(0, totalPages - 1);
  });

  function parseDraft(raw: string, previous: unknown): unknown {
    const t = raw.trim();
    if (t === "" || t.toUpperCase() === "NULL") return null;
    if (typeof previous === "boolean") {
      const lower = t.toLowerCase();
      if (lower === "true" || lower === "1" || lower === "yes") return true;
      if (lower === "false" || lower === "0" || lower === "no") return false;
    }
    if (typeof previous === "number") {
      const n = Number(t);
      if (!Number.isNaN(n)) return n;
    }
    if (
      previous !== null &&
      previous !== undefined &&
      typeof previous !== "boolean" &&
      typeof previous !== "object" &&
      /^-?\d+(\.\d+)?$/.test(t)
    ) {
      const n = Number(t);
      if (!Number.isNaN(n)) return n;
    }
    return raw;
  }

  function startEdit(row: number, col: string, e: MouseEvent) {
    e.stopPropagation();
    onselectrow(row);
    if (editing?.row === row && editing.col === col) return;
    if (editing) commitEdit();
    const value = rows[row]?.[col];
    editing = { row, col };
    draft = value === null || value === undefined ? "" : cellText(value);
  }

  function commitEdit() {
    if (!editing) return;
    const { row, col } = editing;
    const prev = rows[row]?.[col];
    const next = parseDraft(draft, prev);
    rows = rows.map((r, i) => (i === row ? { ...r, [col]: next } : r));
    if (table.rows[row]) {
      table.rows[row] = { ...table.rows[row], [col]: next };
    } else {
      // Ensure array slot exists
      table.rows[row] = { ...(rows[row] ?? {}), [col]: next };
    }

    const key = cellKey(row, col);
    const original = originals[row]?.[col];
    const nextDirty = new Set(dirtyKeys);
    if (valuesEqual(next, original)) nextDirty.delete(key);
    else nextDirty.add(key);
    dirtyKeys = nextDirty;

    editing = null;
    draft = "";
  }

  function cancelEdit() {
    editing = null;
    draft = "";
  }

  function onInputKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      commitEdit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelEdit();
    } else if (e.key === "Tab") {
      commitEdit();
    }
  }

  function onRowClick(absoluteIndex: number) {
    if (editing) commitEdit();
    onselectrow(absoluteIndex);
  }

  function setPageSize(value: string) {
    pageSize = Number(value) || 50;
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
    saving = false;
    statusMsg = "Changes saved";
    onbaselinechange?.();
    setTimeout(() => {
      if (statusMsg === "Changes saved") statusMsg = null;
    }, 2000);
  }

  function resetChanges() {
    if (editing) cancelEdit();
    if (dirtyCount === 0) return;
    const restored = originals.map((r) => ({ ...r }));
    rows = restored.map((r) => ({ ...r }));
    table.rows.splice(0, table.rows.length, ...restored.map((r) => ({ ...r })));
    dirtyKeys = new Set();
    statusMsg = "Changes discarded";
    onbaselinechange?.();
    setTimeout(() => {
      if (statusMsg === "Changes discarded") statusMsg = null;
    }, 2000);
  }
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col">
  <!-- Filters: fixed single-row height so UI ↔ code does not shift layout -->
  <div class="shrink-0 border-b border-line bg-elevated px-2.5 py-2">
    <div class="flex w-full min-w-0 items-center gap-1.5">
      <!-- Mode switch (funnel = UI, code = expression) -->
      <IconButton
        label={filterMode === "ui"
          ? "Switch to code filters"
          : "Switch to UI filters"}
        size="sm"
        variant="ghost"
        active={filterMode === "code"}
        onclick={() => setFilterMode(filterMode === "ui" ? "code" : "ui")}
      >
        {#if filterMode === "ui"}
          <svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path
              d="M2.5 3.5h11l-4 4.5v3.5L6.5 13V8L2.5 3.5z"
              stroke="currentColor"
              stroke-width="1.3"
              stroke-linejoin="round"
            />
          </svg>
        {:else}
          <svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path
              d="M5.5 4.25 2.75 8 5.5 11.75M10.5 4.25 13.25 8 10.5 11.75M9 3.5l-2 9"
              stroke="currentColor"
              stroke-width="1.3"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        {/if}
      </IconButton>

      {#if filterMode === "ui"}
        {@const rule = filterRules[0]}
        {#if rule}
          <div class="w-[15%] min-w-0 shrink-0">
            <Select
              value={rule.column}
              options={columnOptions}
              inputSize="sm"
              placeholder="Column"
              onchange={(v) => updateFirstRule({ column: v })}
            />
          </div>
          <div class="w-[15%] min-w-0 shrink-0">
            <Select
              value={rule.op}
              options={opOptions}
              inputSize="sm"
              placeholder="Operator"
              onchange={(v) => updateFirstRule({ op: v as FilterOp })}
            />
          </div>
          <div class="min-w-0 flex-1">
            <Input
              inputSize="sm"
              class="font-mono"
              placeholder={opNeedsValue(rule.op) ? "Value" : "—"}
              value={rule.value}
              disabled={!opNeedsValue(rule.op)}
              oninput={(e) =>
                updateFirstRule({
                  value: (e.currentTarget as HTMLInputElement).value,
                })}
              onkeydown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  applyFilters();
                }
              }}
            />
          </div>
        {/if}
      {:else}
        <div class="min-w-0 flex-1">
          <Input
            inputSize="sm"
            class="font-mono"
            placeholder={"status = 'pending' AND total_cents > 1000"}
            bind:value={filterCode}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                applyFilters();
              }
            }}
          />
        </div>
      {/if}

      <!-- Trailing actions: same three slots in both modes (stable width/height) -->
      <div class="flex h-control-sm shrink-0 items-center gap-1">
        {#if filterMode === "ui"}
          <IconButton
            label="Remove condition"
            size="sm"
            variant="ghost"
            disabled={filterRules.length <= 1}
            onclick={() => {
              if (filterRules[0]) removeRule(filterRules[0].id);
            }}
          >
            <svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path
                d="M4 4l8 8M12 4l-8 8"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linecap="round"
              />
            </svg>
          </IconButton>
          <IconButton
            label="Add condition"
            size="sm"
            variant="ghost"
            onclick={addRule}
          >
            <svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path
                d="M8 3.25v9.5M3.25 8h9.5"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linecap="round"
              />
            </svg>
          </IconButton>
        {:else}
          <IconButton
            label="Clear filters"
            size="sm"
            variant="ghost"
            onclick={clearFilters}
          >
            <svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path
                d="M4 4l8 8M12 4l-8 8"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linecap="round"
              />
            </svg>
          </IconButton>
          <!-- Placeholder keeps action group width identical to UI mode -->
          <span class="size-control-sm shrink-0" aria-hidden="true"></span>
        {/if}
        <IconButton
          label="Apply filters"
          size="sm"
          variant="accent"
          active
          onclick={applyFilters}
        >
          <svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <circle cx="7" cy="7" r="4.25" stroke="currentColor" stroke-width="1.4" />
            <path
              d="M10.25 10.25 13.5 13.5"
              stroke="currentColor"
              stroke-width="1.4"
              stroke-linecap="round"
            />
          </svg>
        </IconButton>
      </div>
    </div>

    <!-- Extra UI conditions (below first row; does not affect code/UI toggle height) -->
    {#if filterMode === "ui" && filterRules.length > 1}
      <div class="mt-1.5 flex flex-col gap-1.5">
        {#each filterRules.slice(1) as rule (rule.id)}
          <div class="flex w-full min-w-0 items-center gap-1.5">
            <span class="size-control-sm shrink-0" aria-hidden="true"></span>
            <div class="w-[15%] min-w-0 shrink-0">
              <Select
                value={rule.column}
                options={columnOptions}
                inputSize="sm"
                placeholder="Column"
                onchange={(v) => updateRule(rule.id, { column: v })}
              />
            </div>
            <div class="w-[15%] min-w-0 shrink-0">
              <Select
                value={rule.op}
                options={opOptions}
                inputSize="sm"
                placeholder="Operator"
                onchange={(v) =>
                  updateRule(rule.id, { op: v as FilterOp })}
              />
            </div>
            <div class="min-w-0 flex-1">
              <Input
                inputSize="sm"
                class="font-mono"
                placeholder={opNeedsValue(rule.op) ? "Value" : "—"}
                value={rule.value}
                disabled={!opNeedsValue(rule.op)}
                oninput={(e) =>
                  updateRule(rule.id, {
                    value: (e.currentTarget as HTMLInputElement).value,
                  })}
                onkeydown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    applyFilters();
                  }
                }}
              />
            </div>
            <div class="flex h-control-sm shrink-0 items-center gap-1">
              <IconButton
                label="Remove condition"
                size="sm"
                variant="ghost"
                onclick={() => removeRule(rule.id)}
              >
                <svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                  <path
                    d="M4 4l8 8M12 4l-8 8"
                    stroke="currentColor"
                    stroke-width="1.4"
                    stroke-linecap="round"
                  />
                </svg>
              </IconButton>
              <span class="size-control-sm shrink-0" aria-hidden="true"></span>
              <span class="size-control-sm shrink-0" aria-hidden="true"></span>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <div class="min-h-0 flex-1 overflow-auto">
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
          {@const selected = selectedRowIndex === i}
          {@const stripe = i % 2 === 1}
          <tr
            class={cn(
              "cursor-pointer border-b border-line/70 transition-colors",
              selected
                ? "bg-selected"
                : stripe
                  ? "bg-panel hover:bg-hover"
                  : "bg-transparent hover:bg-hover",
            )}
            onclick={() => onRowClick(i)}
            aria-selected={selected}
          >
            <td
              class="px-2 py-1 font-mono text-caption text-fg-muted tabular-nums"
            >
              {i + 1}
            </td>
            {#each table.columns as col (col.name)}
              {@const raw = row[col.name]}
              {@const isNull = raw === null || raw === undefined}
              {@const dirty = isDirty(i, col.name)}
              {@const isEditing =
                editing?.row === i && editing.col === col.name}
              <td
                class={cn(
                  "relative max-w-64 border-l border-transparent p-0 font-mono text-label",
                  dirty &&
                    !isEditing &&
                    "bg-warning/15 ring-1 ring-inset ring-warning/40",
                  isEditing && "bg-input ring-2 ring-inset ring-accent",
                )}
                title={isEditing
                  ? undefined
                  : dirty
                    ? `Edited · original: ${cellText(originals[i]?.[col.name])}`
                    : cellText(raw)}
                ondblclick={(e) => startEdit(i, col.name, e)}
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
                  <input
                    bind:this={inputEl}
                    class={cn(
                      "absolute inset-0 box-border h-full w-full border-none bg-transparent px-2.5 py-1",
                      "font-mono text-label text-fg focus:outline-none",
                    )}
                    bind:value={draft}
                    onkeydown={onInputKeydown}
                    onblur={commitEdit}
                    onclick={(e) => e.stopPropagation()}
                    aria-label="Edit {col.name}"
                  />
                {/if}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>

    {#if pageEntries.length === 0}
      <div class="px-4 py-8 text-center text-caption text-fg-muted">
        {#if filtersActive && activeRules().length > 0}
          No rows match the current filters.
        {:else}
          No rows on this page.
        {/if}
      </div>
    {/if}
  </div>

  <footer
    class="flex h-statusbar shrink-0 items-center gap-2 overflow-x-auto border-t border-line bg-elevated px-2.5"
  >
    <div class="min-w-0 flex-1 basis-32">
      <div
        class="truncate font-mono text-caption font-medium leading-tight text-fg"
        title={tablePath}
      >
        {tablePath}
      </div>
      <div class="truncate text-caption leading-tight text-fg-muted">
        {#if filteredCount === 0}
          No sample rows
        {:else}
          {pageStart + 1}–{pageEnd} of {filteredCount}
          {#if filtersActive && activeRules().length > 0}
            filtered
          {:else}
            sample
          {/if}
        {/if}
        <span aria-hidden="true"> · </span>
        ~{table.rowCount.toLocaleString()} rows
        {#if dirtyCount > 0}
          <span
            class="ml-1 rounded-sm border border-warning/45 bg-warning/15 px-1 py-px font-medium text-warning"
          >
            {dirtyCount} unsaved
          </span>
        {/if}
        {#if statusMsg}
          <span class="ml-1 text-success">{statusMsg}</span>
        {/if}
      </div>
    </div>

    <div class="flex shrink-0 items-center gap-1.5">
      <div class="w-28">
        <Select
          id="grid-page-size"
          value={String(pageSize)}
          options={pageSizeOptions}
          inputSize="sm"
          onchange={setPageSize}
        />
      </div>

      <div
        class="flex items-center gap-0.5 rounded-md border border-line bg-input p-0.5"
        role="group"
        aria-label="Pagination"
      >
        <Button
          variant="ghost"
          size="sm"
          disabled={safePage <= 0}
          onclick={goPrev}
          aria-label="Previous page"
        >
          ‹
        </Button>
        <span
          class="min-w-14 px-1 text-center font-mono text-caption text-fg-secondary tabular-nums"
        >
          {safePage + 1}/{totalPages}
        </span>
        <Button
          variant="ghost"
          size="sm"
          disabled={safePage >= totalPages - 1}
          onclick={goNext}
          aria-label="Next page"
        >
          ›
        </Button>
      </div>

      <Button variant="ghost" size="sm" disabled={refreshing} onclick={refresh}>
        {refreshing ? "Refreshing…" : "Refresh"}
      </Button>

      <Button
        variant="ghost"
        size="sm"
        disabled={dirtyCount === 0 || saving}
        onclick={resetChanges}
      >
        Reset changes
      </Button>

      <Button
        variant="primary"
        size="sm"
        disabled={dirtyCount === 0 || saving}
        onclick={saveChanges}
      >
        {saving ? "Saving…" : "Save changes"}
      </Button>
    </div>
  </footer>
</div>
