<script lang="ts">
  /**
   * Query space: Run → BE execute → results table.
   * Persists runs to query history. Editor supports undo/redo + select all.
   */
  import { Button, Badge, Checkbox } from "$lib/components/ui";
  import { engineLabel } from "$lib/types/connection";
  import {
    executeEngineQuery,
    type QueryResult,
  } from "$lib/api/engines";
  import {
    appendQueryHistory,
    type QueryHistoryItem,
  } from "$lib/api/history";
  import { isTauri } from "$lib/api/tauri";
  import {
    queryLanguageForEngine,
    queryPlaceholder,
    type QueryMode,
  } from "$lib/utils/queryPolicyFe";
  import {
    exportRowsAsCsv,
    exportRowsAsPdf,
  } from "$lib/utils/exportTable";
  import { cn } from "$lib/utils/cn";

  const RESULT_PAGE_SIZE = 25;

  interface Props {
    connectionId: string;
    connectionName: string;
    engine: string;
    profileReadOnly: boolean;
    engineSessionId?: string | null;
    /** Selected database from the explorer (Mongo multi-DB). */
    database?: string | null;
    text?: string;
    writeEnabled?: boolean;
    ontextchange?: (text: string) => void;
    onwritechange?: (enabled: boolean) => void;
    /**
     * Fired after a run is recorded (success or failure).
     * Receives the persisted history row when available.
     */
    onhistory?: (item?: QueryHistoryItem | null) => void;
  }

  let {
    connectionId,
    connectionName,
    engine,
    profileReadOnly,
    engineSessionId = null,
    database = null,
    text = "",
    writeEnabled = false,
    ontextchange,
    onwritechange,
    onhistory,
  }: Props = $props();

  type RunState =
    | { status: "idle" }
    | { status: "running" }
    | {
        status: "ok" | "blocked" | "error";
        message: string;
        mode: QueryMode;
        elapsedMs: number;
        result?: QueryResult | null;
      };

  let runState = $state<RunState>({ status: "idle" });
  let collection = $state("");
  let editorEl = $state<HTMLTextAreaElement | null>(null);
  /** 0-based page within the current result set. */
  let resultPage = $state(0);
  let exporting = $state<"csv" | "pdf" | null>(null);
  let exportMsg = $state<string | null>(null);

  /**
   * Editor undo/redo.
   * Controlled textarea breaks the browser's native undo, so we own the stacks.
   * Snapshot = editor content *before* a change.
   */
  const MAX_UNDO = 100;
  let undoStack = $state<string[]>([]);
  let redoStack = $state<string[]>([]);
  /** Skip recording while applying undo/redo. */
  let applyingHistory = false;

  const language = $derived(queryLanguageForEngine(engine));
  const placeholder = $derived(queryPlaceholder(engine));
  const effectiveMode = $derived<QueryMode>(
    profileReadOnly || !writeEnabled ? "read" : "write",
  );
  const canUndo = $derived(undoStack.length > 0);
  const canRedo = $derived(redoStack.length > 0);

  const resultRows = $derived(
    runState.status === "ok" && runState.result ? runState.result.rows : [],
  );
  const resultColumns = $derived(
    runState.status === "ok" && runState.result ? runState.result.columns : [],
  );
  const resultTotal = $derived(resultRows.length);
  const resultTotalPages = $derived(
    Math.max(1, Math.ceil(resultTotal / RESULT_PAGE_SIZE) || 1),
  );
  const resultSafePage = $derived(
    Math.min(resultPage, resultTotalPages - 1),
  );
  const resultPageStart = $derived(resultSafePage * RESULT_PAGE_SIZE);
  const resultPageEnd = $derived(
    Math.min(resultPageStart + RESULT_PAGE_SIZE, resultTotal),
  );
  const resultPageRows = $derived(
    resultRows.slice(resultPageStart, resultPageEnd),
  );
  const canExport = $derived(
    runState.status === "ok" &&
      !!runState.result &&
      runState.result.columns.length > 0 &&
      runState.result.rows.length > 0,
  );

  $effect(() => {
    if (profileReadOnly && writeEnabled) {
      onwritechange?.(false);
    }
  });

  function pushUndoSnapshot(snapshot: string) {
    if (applyingHistory) return;
    // Avoid consecutive duplicate snapshots
    if (undoStack.length > 0 && undoStack[undoStack.length - 1] === snapshot) {
      return;
    }
    undoStack = [...undoStack.slice(-(MAX_UNDO - 1)), snapshot];
    redoStack = [];
  }

  function applyEditorText(next: string) {
    applyingHistory = true;
    ontextchange?.(next);
    // Release on next microtask so parent can update `text` prop first
    queueMicrotask(() => {
      applyingHistory = false;
    });
  }

  function undo() {
    if (undoStack.length === 0) return;
    const prev = undoStack[undoStack.length - 1]!;
    undoStack = undoStack.slice(0, -1);
    redoStack = [...redoStack, text];
    applyEditorText(prev);
    // Restore caret to end after undo
    queueMicrotask(() => {
      const el = editorEl;
      if (!el) return;
      el.focus();
      const n = prev.length;
      el.setSelectionRange(n, n);
    });
  }

  function redo() {
    if (redoStack.length === 0) return;
    const next = redoStack[redoStack.length - 1]!;
    redoStack = redoStack.slice(0, -1);
    if (undoStack.length === 0 || undoStack[undoStack.length - 1] !== text) {
      undoStack = [...undoStack.slice(-(MAX_UNDO - 1)), text];
    }
    applyEditorText(next);
    queueMicrotask(() => {
      const el = editorEl;
      if (!el) return;
      el.focus();
      const n = next.length;
      el.setSelectionRange(n, n);
    });
  }

  function selectAll() {
    const el = editorEl;
    if (!el) return;
    el.focus();
    el.select();
  }

  function onToggleWrite(checked: boolean) {
    if (profileReadOnly) return;
    onwritechange?.(checked);
  }

  function onTextInput(value: string) {
    if (applyingHistory) {
      ontextchange?.(value);
      return;
    }
    if (value !== text) {
      pushUndoSnapshot(text);
    }
    ontextchange?.(value);
  }

  function cellDisplay(v: unknown): string {
    if (v === null || v === undefined) return "NULL";
    if (typeof v === "object") {
      try {
        return JSON.stringify(v);
      } catch {
        return String(v);
      }
    }
    return String(v);
  }

  function goResultPrev() {
    resultPage = Math.max(0, resultSafePage - 1);
  }

  function goResultNext() {
    resultPage = Math.min(resultTotalPages - 1, resultSafePage + 1);
  }

  async function exportResults(format: "csv" | "pdf") {
    if (!canExport || !runState.result || exporting) return;
    exporting = format;
    exportMsg = null;
    const cols = runState.result.columns.map((c) => ({ name: c.name }));
    const rows = runState.result.rows;
    const fileBase = `${connectionName}-${language}-results`.replace(
      /\s+/g,
      "-",
    );
    try {
      const res =
        format === "csv"
          ? await exportRowsAsCsv(cols, rows, fileBase)
          : await exportRowsAsPdf(cols, rows, {
              title: `${connectionName} · ${language.toUpperCase()} results`,
              fileBase,
            });
      if (res.ok) {
        exportMsg = format === "csv" ? "CSV exported" : "PDF exported";
      } else if (!res.cancelled) {
        exportMsg = res.error;
      }
    } catch (e) {
      exportMsg = e instanceof Error ? e.message : String(e);
    } finally {
      exporting = null;
      setTimeout(() => {
        if (exportMsg && !exportMsg.toLowerCase().includes("fail") && !exportMsg.toLowerCase().includes("error")) {
          exportMsg = null;
        }
      }, 2500);
    }
  }

  /** Prefer live textarea content so history always matches what was run. */
  function currentQueryText(): string {
    const fromEditor = editorEl?.value;
    if (typeof fromEditor === "string" && fromEditor.length > 0) {
      return fromEditor;
    }
    return text ?? "";
  }

  async function recordHistory(opts: {
    success: boolean;
    durationMs: number;
    rowCount?: number | null;
    errorMessage?: string | null;
    body?: string;
  }): Promise<QueryHistoryItem | null> {
    if (!isTauri()) return null;
    const body = (opts.body ?? currentQueryText()).trim();
    if (!body) return null;
    try {
      const item = await appendQueryHistory({
        connectionId,
        engine,
        body,
        language: language === "mql" ? "mql" : "sql",
        success: opts.success,
        rowCount: opts.rowCount ?? null,
        durationMs: opts.durationMs,
        errorMessage: opts.errorMessage ?? null,
      });
      onhistory?.(item);
      return item;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.warn("[mystique] failed to append query history:", msg);
      // Still notify parent so it can refresh from SQLite if something was written.
      onhistory?.(null);
      return null;
    }
  }

  async function runQuery() {
    const mode = effectiveMode;
    const start = performance.now();
    resultPage = 0;
    exportMsg = null;
    runState = { status: "running" };
    const sqlText = currentQueryText();

    if (!isTauri()) {
      runState = {
        status: "error",
        message:
          "Live query execute requires Tauri. Run `npm run tauri dev` and Connect again.",
        mode,
        elapsedMs: Math.round(performance.now() - start),
      };
      return;
    }

    if (!engineSessionId) {
      const elapsedMs = Math.round(performance.now() - start);
      runState = {
        status: "error",
        message:
          "No live engine session. Go Home → Connect so a pooled session is opened.",
        mode,
        elapsedMs,
      };
      await recordHistory({
        success: false,
        durationMs: elapsedMs,
        errorMessage: "No live engine session",
        body: sqlText,
      });
      return;
    }

    try {
      const result = await executeEngineQuery({
        sessionId: engineSessionId,
        text: sqlText,
        mode,
        maxRows: 500,
        timeoutMs: 30000,
        profileReadOnly,
        collection: language === "mql" ? collection.trim() || null : null,
        database:
          language === "mql" && database?.trim()
            ? database.trim()
            : null,
      });
      const elapsedMs = Math.round(performance.now() - start);
      const note = result.message ? ` · ${result.message}` : "";
      const trunc = result.truncated ? " · truncated" : "";
      runState = {
        status: "ok",
        message: `${result.rowCount} row(s) · ${result.durationMs}ms${trunc}${note}`,
        mode,
        elapsedMs,
        result,
      };
      await recordHistory({
        success: true,
        durationMs: result.durationMs ?? elapsedMs,
        rowCount: result.rowCount,
        body: sqlText,
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      const isPolicy = /policy:/i.test(msg) || /blocked/i.test(msg);
      const elapsedMs = Math.round(performance.now() - start);
      runState = {
        status: isPolicy ? "blocked" : "error",
        message: msg.replace(/^policy:\s*/i, ""),
        mode,
        elapsedMs,
      };
      await recordHistory({
        success: false,
        durationMs: elapsedMs,
        errorMessage: msg,
        body: sqlText,
      });
    }
  }

  function isMod(e: KeyboardEvent) {
    return e.ctrlKey || e.metaKey;
  }

  function onKeydown(e: KeyboardEvent) {
    if (!isMod(e)) return;

    // Prefer e.code so layout/shift quirks don't break Z/Y/A
    const code = e.code;
    const key = e.key.toLowerCase();

    if (code === "Enter" || key === "enter") {
      e.preventDefault();
      e.stopPropagation();
      void runQuery();
      return;
    }
    if (code === "KeyA" || key === "a") {
      e.preventDefault();
      e.stopPropagation();
      selectAll();
      return;
    }
    // Undo: Ctrl+Z (no Shift)
    if ((code === "KeyZ" || key === "z") && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      e.stopPropagation();
      undo();
      return;
    }
    // Redo: Ctrl+Y or Ctrl+Shift+Z
    if (
      code === "KeyY" ||
      key === "y" ||
      ((code === "KeyZ" || key === "z") && e.shiftKey)
    ) {
      e.preventDefault();
      e.stopPropagation();
      redo();
      return;
    }
  }

  /** WebKit often fires historyUndo/historyRedo on controlled fields — intercept. */
  function onBeforeInput(e: InputEvent) {
    if (e.inputType === "historyUndo") {
      e.preventDefault();
      undo();
      return;
    }
    if (e.inputType === "historyRedo") {
      e.preventDefault();
      redo();
    }
  }
</script>

<div
  class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash"
  data-connection-id={connectionId}
>
  <div
    class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-line bg-elevated px-3 py-2"
  >
    <div class="min-w-0">
      <div class="flex flex-wrap items-center gap-2">
        <span class="truncate text-body font-semibold text-fg">Query</span>
        <Badge tone="neutral">{engineLabel(engine)}</Badge>
        <Badge tone={language === "sql" ? "info" : "warning"}>
          {language.toUpperCase()}
        </Badge>
        <span class="truncate text-caption text-fg-muted">{connectionName}</span>
        {#if engineSessionId}
          <Badge tone="success">live</Badge>
        {:else}
          <Badge tone="warning">no session</Badge>
        {/if}
      </div>
      <p class="mt-0.5 text-caption text-fg-muted">
        {#if profileReadOnly}
          Connection is read-only · writes disabled
        {:else if effectiveMode === "read"}
          Read mode · enable writes for INSERT/UPDATE (DELETE blocked)
        {:else}
          Write mode · DELETE still blocked
        {/if}
        <span class="text-fg-muted"> · Ctrl+Z/Y undo · Ctrl+A select all</span>
      </p>
    </div>

    <div class="flex shrink-0 flex-wrap items-center gap-2">
      {#if language === "mql"}
        <input
          class="h-control-sm w-36 rounded-md border border-line bg-input px-2 font-mono text-caption text-fg"
          placeholder="collection"
          bind:value={collection}
          title="MongoDB collection name"
        />
      {/if}
      <Button
        variant="ghost"
        size="sm"
        disabled={!canUndo}
        onclick={() => undo()}
        title="Undo (Ctrl+Z)"
      >
        Undo
      </Button>
      <Button
        variant="ghost"
        size="sm"
        disabled={!canRedo}
        onclick={() => redo()}
        title="Redo (Ctrl+Y)"
      >
        Redo
      </Button>
      <label
        class={cn(
          "inline-flex items-center gap-2 rounded-md border px-2 py-1 text-label",
          profileReadOnly
            ? "cursor-not-allowed border-line bg-input text-fg-muted opacity-70"
            : effectiveMode === "write"
              ? "border-warning/50 bg-warning/10 text-fg"
              : "border-line bg-input text-fg-secondary",
        )}
      >
        <Checkbox
          checked={writeEnabled && !profileReadOnly}
          disabled={profileReadOnly}
          onchange={(e) =>
            onToggleWrite((e.currentTarget as HTMLInputElement).checked)}
          aria-label="Allow writes"
        />
        <span class="whitespace-nowrap">Allow writes</span>
      </label>

      <Button
        variant="primary"
        size="sm"
        disabled={runState.status === "running" || !text.trim()}
        onclick={() => void runQuery()}
      >
        {runState.status === "running" ? "Running…" : "Run"}
      </Button>
    </div>
  </div>

  <div class="flex min-h-0 flex-1 flex-col">
    <div class="flex min-h-0 min-w-0 flex-[0_0_42%] flex-col border-b border-line">
      <label class="sr-only" for="query-editor-{connectionId}">Query editor</label>
      <textarea
        id="query-editor-{connectionId}"
        bind:this={editorEl}
        class={cn(
          "min-h-32 w-full flex-1 resize-none border-none bg-panel p-3",
          "font-mono text-body leading-relaxed text-fg",
          "placeholder:text-fg-muted focus-visible:outline-none",
        )}
        {placeholder}
        spellcheck="false"
        value={text}
        oninput={(e) => onTextInput((e.currentTarget as HTMLTextAreaElement).value)}
        onkeydown={onKeydown}
        onbeforeinput={onBeforeInput}
      ></textarea>
      <div
        class="flex h-statusbar shrink-0 items-center justify-between gap-2 border-t border-line bg-elevated px-2.5 text-caption text-fg-muted"
      >
        <span>⌘/Ctrl+Enter run · Ctrl+Z/Y undo/redo · Ctrl+A select all</span>
        <span class="font-mono tabular-nums">{text.length} chars</span>
      </div>
    </div>

    <div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
      <div
        class="flex h-control-sm shrink-0 items-center justify-between gap-2 border-b border-line bg-elevated px-3"
      >
        <div class="flex min-w-0 items-center gap-2">
          <span class="text-label font-medium text-fg-secondary">Results</span>
          {#if runState.status === "ok" || runState.status === "blocked" || runState.status === "error"}
            <span
              class={cn(
                "truncate text-caption",
                runState.status === "ok" && "text-success",
                runState.status !== "ok" && "text-danger",
              )}
              title={runState.message}
            >
              {runState.message}
            </span>
          {/if}
          {#if exportMsg}
            <span class="truncate text-caption text-fg-muted" title={exportMsg}
              >{exportMsg}</span
            >
          {/if}
        </div>
        <div class="flex shrink-0 items-center gap-1.5">
          <Button
            variant="ghost"
            size="sm"
            disabled={!canExport || exporting !== null}
            onclick={() => void exportResults("csv")}
            title="Export all result rows as CSV"
          >
            {exporting === "csv" ? "CSV…" : "CSV"}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={!canExport || exporting !== null}
            onclick={() => void exportResults("pdf")}
            title="Export all result rows as PDF"
          >
            {exporting === "pdf" ? "PDF…" : "PDF"}
          </Button>
        </div>
      </div>

      <div class="min-h-0 flex-1 overflow-auto">
        {#if runState.status === "idle"}
          <p class="p-3 text-body text-fg-muted">
            Write a query and press <strong class="font-medium text-fg-secondary">Run</strong>.
            Runs are saved to History.
          </p>
        {:else if runState.status === "running"}
          <p class="p-3 text-body text-fg-muted">Running…</p>
        {:else if runState.status === "ok" && runState.result}
          {@const result = runState.result}
          {#if result.columns.length === 0}
            <p class="p-3 text-body text-fg-muted">
              {result.message ?? "Statement finished with no tabular rows."}
            </p>
          {:else if resultTotal === 0}
            <p class="p-3 text-body text-fg-muted">No rows returned.</p>
          {:else}
            <table class="w-max min-w-full border-collapse text-left">
              <thead class="sticky top-0 z-10 bg-elevated">
                <tr>
                  <th
                    class="border-b border-line px-2 py-1.5 font-mono text-caption font-semibold text-fg-muted"
                    >#</th
                  >
                  {#each resultColumns as col}
                    <th
                      class="border-b border-line px-2.5 py-1.5 font-mono text-caption font-semibold whitespace-nowrap text-fg-secondary"
                      title={col.dataType ?? undefined}
                    >
                      {col.name}
                      {#if col.dataType}
                        <span class="ml-1 font-normal text-fg-muted">{col.dataType}</span>
                      {/if}
                    </th>
                  {/each}
                </tr>
              </thead>
              <tbody>
                {#each resultPageRows as row, localRi}
                  {@const ri = resultPageStart + localRi}
                  <tr class="hover:bg-hover">
                    <td
                      class="border-b border-line/60 px-2 py-1 font-mono text-caption text-fg-muted tabular-nums"
                      >{ri + 1}</td
                    >
                    {#each row as cell}
                      {@const isNull = cell === null || cell === undefined}
                      <td
                        class={cn(
                          "max-w-xs truncate border-b border-line/60 px-2.5 py-1 font-mono text-label",
                          isNull ? "italic text-fg-muted" : "text-fg",
                        )}
                        title={cellDisplay(cell)}
                      >
                        {cellDisplay(cell)}
                      </td>
                    {/each}
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        {:else}
          <div
            class="m-3 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-label text-danger"
            role="status"
          >
            {runState.status === "blocked" || runState.status === "error"
              ? runState.message
              : ""}
          </div>
        {/if}
      </div>

      {#if canExport}
        <div
          class="flex h-statusbar shrink-0 items-center justify-between gap-2 border-t border-line bg-elevated px-2.5 text-caption text-fg-muted"
        >
          <span class="font-mono tabular-nums">
            {resultTotal === 0
              ? "0 rows"
              : `${resultPageStart + 1}–${resultPageEnd} of ${resultTotal}`}
            · {RESULT_PAGE_SIZE} / page
          </span>
          <div class="flex items-center gap-1">
            <Button
              variant="ghost"
              size="sm"
              disabled={resultSafePage <= 0}
              onclick={goResultPrev}
            >
              Prev
            </Button>
            <span class="min-w-[4.5rem] text-center font-mono tabular-nums">
              {resultSafePage + 1} / {resultTotalPages}
            </span>
            <Button
              variant="ghost"
              size="sm"
              disabled={resultSafePage >= resultTotalPages - 1}
              onclick={goResultNext}
            >
              Next
            </Button>
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>
