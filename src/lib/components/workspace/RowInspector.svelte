<script lang="ts">
  import type { ColumnDef } from "$lib/types/workspace";
  import { IconButton } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";
  import {
    selectDraftToValue,
    selectOptionsForColumn,
    usesSelectEditor,
    valueToSelectDraft,
  } from "$lib/utils/columnEditor";

  interface Props {
    tableTitle: string;
    columns: ColumnDef[];
    row: Record<string, unknown>;
    rowIndex: number;
    onclose: () => void;
    /** Commit a field edit (parsed value). */
    onchange?: (column: string, value: unknown) => void;
    /** Bumps on grid save / reset / refresh so dirty baseline re-captures. */
    baselineEpoch?: number;
  }

  let {
    tableTitle,
    columns,
    row,
    rowIndex,
    onclose,
    onchange,
    baselineEpoch = 0,
  }: Props = $props();

  /** Values at last baseline (selection open or after save/reset). */
  let baseline = $state<Record<string, unknown>>({});
  /** String drafts shown in inputs. */
  let drafts = $state<Record<string, string>>({});
  let focusedCol = $state<string | null>(null);

  function formatForEdit(col: ColumnDef, value: unknown): string {
    if (usesSelectEditor(col, value)) return valueToSelectDraft(value);
    if (value === null || value === undefined) return "";
    if (typeof value === "boolean") return value ? "true" : "false";
    if (typeof value === "object") return JSON.stringify(value, null, 2);
    return String(value);
  }

  function valuesEqual(a: unknown, b: unknown): boolean {
    if (a === b) return true;
    if (a == null && b == null) return true;
    return String(a) === String(b);
  }

  function parseDraft(
    col: ColumnDef,
    raw: string,
    previous: unknown,
  ): unknown {
    if (usesSelectEditor(col, previous)) {
      return selectDraftToValue(raw, col, previous);
    }
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
    if (
      (t.startsWith("{") && t.endsWith("}")) ||
      (t.startsWith("[") && t.endsWith("]"))
    ) {
      try {
        return JSON.parse(t);
      } catch {
        /* keep string */
      }
    }
    return raw;
  }

  function useTextarea(col: ColumnDef, value: unknown): boolean {
    if (usesSelectEditor(col, value)) return false;
    const t = col.type.toLowerCase();
    if (t.includes("json") || t.includes("text") || t.includes("object")) {
      return true;
    }
    if (t.includes("varchar") && formatForEdit(col, value).length > 48) {
      return true;
    }
    if (typeof value === "object" && value !== null) return true;
    return formatForEdit(col, value).length > 64;
  }

  function captureBaselineFromRow() {
    const nextBaseline: Record<string, unknown> = {};
    const nextDrafts: Record<string, string> = {};
    for (const col of columns) {
      nextBaseline[col.name] = row[col.name];
      nextDrafts[col.name] = formatForEdit(col, row[col.name]);
    }
    baseline = nextBaseline;
    drafts = nextDrafts;
    focusedCol = null;
  }

  // New row / table, or save-baseline epoch → reset drafts & baseline
  let lastBaselineKey = $state("");
  $effect(() => {
    const key = `${tableTitle}::${rowIndex}::${baselineEpoch}`;
    if (key === lastBaselineKey) return;
    lastBaselineKey = key;
    captureBaselineFromRow();
  });

  // Sync drafts from row when grid edits land (skip while typing)
  $effect(() => {
    if (focusedCol) return;
    // Depend on row field values
    for (const col of columns) {
      void row[col.name];
    }
    const next: Record<string, string> = {};
    let changed = false;
    for (const col of columns) {
      const v = formatForEdit(col, row[col.name]);
      next[col.name] = v;
      if (drafts[col.name] !== v) changed = true;
    }
    if (changed) drafts = next;
  });

  function isDirty(colName: string): boolean {
    const col = columns.find((c) => c.name === colName);
    if (!col) return false;
    const parsed = parseDraft(col, drafts[colName] ?? "", baseline[colName]);
    return !valuesEqual(parsed, baseline[colName]);
  }

  function commitField(colName: string) {
    const col = columns.find((c) => c.name === colName);
    if (!col) return;
    const previous = row[colName];
    const next = parseDraft(col, drafts[colName] ?? "", previous);
    drafts = { ...drafts, [colName]: formatForEdit(col, next) };
    if (!valuesEqual(next, previous)) {
      onchange?.(colName, next);
    }
    focusedCol = null;
  }

  function onFieldKeydown(
    e: KeyboardEvent,
    colName: string,
    multiline: boolean,
  ) {
    if (e.key === "Escape") {
      e.preventDefault();
      const col = columns.find((c) => c.name === colName);
      drafts = {
        ...drafts,
        [colName]: col
          ? formatForEdit(col, row[colName])
          : String(row[colName] ?? ""),
      };
      focusedCol = null;
      (e.currentTarget as HTMLElement).blur();
      return;
    }
    if (e.key === "Enter" && !multiline && !e.shiftKey) {
      e.preventDefault();
      commitField(colName);
      (e.currentTarget as HTMLElement).blur();
    }
  }

  const dirtyCount = $derived(
    columns.reduce((n, c) => n + (isDirty(c.name) ? 1 : 0), 0),
  );
</script>

<aside
  class="flex h-full w-72 shrink-0 flex-col border-l border-line bg-panel"
  aria-label="Row detail"
>
  <header
    class="flex h-topbar shrink-0 items-center justify-between gap-2 border-b border-line px-3"
  >
    <div class="min-w-0">
      <div class="flex items-center gap-1.5">
        <span class="truncate text-label font-semibold text-fg">
          Row {rowIndex + 1}
        </span>
        {#if dirtyCount > 0}
          <span
            class="shrink-0 rounded-sm border border-warning/45 bg-warning/15 px-1 py-px text-micro font-medium text-warning"
          >
            {dirtyCount} edited
          </span>
        {/if}
      </div>
      <div class="truncate font-mono text-caption text-fg-muted">
        {tableTitle}
      </div>
    </div>
    <IconButton
      label="Close row inspector"
      size="sm"
      variant="ghost"
      onclick={onclose}
    >
      ×
    </IconButton>
  </header>

  <div class="flex-1 overflow-auto p-2">
    <p class="mb-2 px-0.5 text-caption text-fg-muted">
      Edit fields below. Enter commits · Esc cancels.
    </p>
    <div class="flex flex-col gap-1.5">
      {#each columns as col (col.name)}
        {@const raw = row[col.name]}
        {@const dirty = isDirty(col.name)}
        {@const multiline = useTextarea(col, raw)}
        {@const focused = focusedCol === col.name}
        <label
          class={cn(
            "block rounded-md border px-2.5 py-2 transition-colors",
            dirty
              ? "border-warning/45 bg-warning/10"
              : focused
                ? "border-focus bg-input ring-2 ring-accent-muted"
                : "border-line bg-elevated",
          )}
        >
          <span
            class="mb-1 flex items-center gap-1.5 text-caption font-medium text-fg-secondary"
          >
            <span class="text-fg">{col.name}</span>
            {#if col.pk}
              <span class="text-accent" title="Primary key">PK</span>
            {/if}
            <span class="font-mono font-normal text-fg-muted">{col.type}</span>
            {#if dirty}
              <span
                class="ml-auto size-1.5 shrink-0 rounded-full bg-warning"
                title="Edited"
                aria-label="Edited"
              ></span>
            {/if}
          </span>

          {#if usesSelectEditor(col, raw)}
            {@const options = selectOptionsForColumn(col, raw)}
            <select
              class={cn(
                "m-0 w-full rounded-sm border border-line bg-input px-1.5 py-1",
                "font-mono text-label text-fg focus:outline-none focus:ring-1 focus:ring-accent",
                (drafts[col.name] ?? "") === "" && "italic text-fg-muted",
              )}
              value={drafts[col.name] ?? ""}
              onfocus={() => (focusedCol = col.name)}
              onchange={(e) => {
                drafts = {
                  ...drafts,
                  [col.name]: (e.currentTarget as HTMLSelectElement).value,
                };
                commitField(col.name);
              }}
              onblur={() => {
                if (focusedCol === col.name) commitField(col.name);
              }}
              onkeydown={(e) => onFieldKeydown(e, col.name, false)}
              aria-label={col.name}
            >
              {#each options as opt (opt.value === "" ? "__null__" : opt.value)}
                <option value={opt.value}>{opt.label}</option>
              {/each}
            </select>
          {:else if multiline}
            <textarea
              class={cn(
                "m-0 min-h-16 w-full resize-y rounded-sm border-none bg-transparent p-0",
                "font-mono text-label leading-relaxed text-fg",
                "focus:outline-none",
                (drafts[col.name] ?? "") === "" && "italic text-fg-muted",
              )}
              rows="3"
              value={drafts[col.name] ?? ""}
              placeholder="NULL"
              onfocus={() => (focusedCol = col.name)}
              oninput={(e) => {
                drafts = {
                  ...drafts,
                  [col.name]: (e.currentTarget as HTMLTextAreaElement).value,
                };
              }}
              onblur={() => commitField(col.name)}
              onkeydown={(e) => onFieldKeydown(e, col.name, true)}
              spellcheck="false"
            ></textarea>
          {:else}
            <input
              class={cn(
                "m-0 w-full border-none bg-transparent p-0 font-mono text-label text-fg",
                "focus:outline-none",
                (drafts[col.name] ?? "") === "" && "italic text-fg-muted",
              )}
              value={drafts[col.name] ?? ""}
              placeholder="NULL"
              onfocus={() => (focusedCol = col.name)}
              oninput={(e) => {
                drafts = {
                  ...drafts,
                  [col.name]: (e.currentTarget as HTMLInputElement).value,
                };
              }}
              onblur={() => commitField(col.name)}
              onkeydown={(e) => onFieldKeydown(e, col.name, false)}
              spellcheck="false"
              autocomplete="off"
            />
          {/if}
        </label>
      {/each}
    </div>
  </div>

  <footer
    class="flex h-statusbar shrink-0 items-center border-t border-line bg-elevated px-2.5 text-caption text-fg-muted"
  >
    {#if dirtyCount > 0}
      <span class="text-warning"
        >{dirtyCount} field{dirtyCount === 1 ? "" : "s"} changed</span
      >
      <span class="mx-1.5 text-fg-muted">·</span>
      <span>Save from the grid bar</span>
    {:else}
      <span>No local field changes</span>
    {/if}
  </footer>
</aside>
