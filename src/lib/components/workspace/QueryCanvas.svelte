<script lang="ts">
  /**
   * Query space: Run → BE execute → results table.
   * Composes toolbar / editor / results; native paste via bind:value.
   */
  import { onMount } from "svelte";
  import { executeEngineQuery } from "$lib/api/engines";
  import { appendQueryHistory, type QueryHistoryItem } from "$lib/api/history";
  import { isTauri } from "$lib/api/tauri";
  import { isSessionMissingError } from "./shell/sessionUtils";
  import {
    queryLanguageForEngine,
    queryPlaceholder,
    type QueryMode,
  } from "$lib/utils/queryPolicyFe";
  import {
    buildCsv,
    buildJson,
    copyTextToClipboard,
    exportFileBaseFromQuery,
    exportRowsAsCsv,
    exportRowsAsExcel,
    exportRowsAsJson,
  } from "$lib/utils/exportTable";
  import QueryEditor from "./query/QueryEditor.svelte";
  import QueryResultsPanel, {
    type ResultExportAction,
  } from "./query/QueryResultsPanel.svelte";
  import {
    QUERY_FETCH_MAX_ROWS,
    RESULT_PAGE_SIZE_DEFAULT,
    type RunState,
  } from "./query/types";

  interface Props {
    connectionId: string;
    connectionName: string;
    engine: string;
    profileReadOnly: boolean;
    engineSessionId?: string | null;
    database?: string | null;
    text?: string;
    writeEnabled?: boolean;
    dirty?: boolean;
    saving?: boolean;
    ontextchange?: (text: string) => void;
    onwritechange?: (enabled: boolean) => void;
    onhistory?: (item?: QueryHistoryItem | null) => void;
    onsave?: () => void;
    /** Re-open a dead engine session; returns the new session id. */
    onreconnect?: () => Promise<string | null>;
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
    dirty = false,
    saving = false,
    ontextchange,
    onwritechange,
    onhistory,
    onsave,
    onreconnect,
  }: Props = $props();

  let runState = $state<RunState>({ status: "idle" });
  let collection = $state("");
  let resultPage = $state(0);
  const resultPageSize = RESULT_PAGE_SIZE_DEFAULT;
  let exporting = $state<ResultExportAction | null>(null);
  let exportMsg = $state<string | null>(null);

  let localText = $state(text ?? "");
  let lastEmitted = text ?? "";

  /** Vertical split: editor pane share (results get the rest). */
  const SPLIT_MIN = 18;
  const SPLIT_MAX = 82;
  const SPLIT_DEFAULT = 42;
  let editorSharePct = $state(SPLIT_DEFAULT);
  let splitRootEl = $state<HTMLDivElement | null>(null);
  let splitDragging = $state(false);

  const language = $derived(queryLanguageForEngine(engine));
  const placeholder = $derived(queryPlaceholder(engine));
  const effectiveMode = $derived<QueryMode>(
    profileReadOnly || !writeEnabled ? "read" : "write",
  );

  const lineCount = $derived.by(() => {
    if (!localText) return 1;
    return localText.split("\n").length;
  });

  // Sync parent draft (e.g. open from history) when not typing in the editor.
  $effect(() => {
    const prop = text ?? "";
    if (prop === lastEmitted || prop === localText) return;
    if (typeof document !== "undefined") {
      const ae = document.activeElement;
      if (ae?.closest?.(`[data-query-editor="${connectionId}"]`)) {
        return;
      }
    }
    localText = prop;
    lastEmitted = prop;
  });

  const resultRows = $derived(
    runState.status === "ok" && runState.result ? runState.result.rows : [],
  );
  const resultColumns = $derived(
    runState.status === "ok" && runState.result ? runState.result.columns : [],
  );
  const resultTotal = $derived(resultRows.length);
  const resultTotalPages = $derived(
    Math.max(1, Math.ceil(resultTotal / resultPageSize) || 1),
  );
  const resultSafePage = $derived(Math.min(resultPage, resultTotalPages - 1));
  const resultPageStart = $derived(resultSafePage * resultPageSize);
  const resultPageEnd = $derived(
    Math.min(resultPageStart + resultPageSize, resultTotal),
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

  function onToggleWrite(checked: boolean) {
    if (profileReadOnly) return;
    onwritechange?.(checked);
  }

  function emitText(value: string) {
    localText = value;
    lastEmitted = value;
    ontextchange?.(value);
  }

  function goResultPrev() {
    resultPage = Math.max(0, resultSafePage - 1);
  }

  function goResultNext() {
    resultPage = Math.min(resultTotalPages - 1, resultSafePage + 1);
  }

  async function exportResults(action: ResultExportAction) {
    if (exporting) return;
    if (runState.status !== "ok" || !runState.result) return;
    const result = runState.result;
    if (result.columns.length === 0 || result.rows.length === 0) return;
    exporting = action;
    exportMsg = null;
    const cols = result.columns.map((c) => ({ name: c.name }));
    const rows = result.rows;
    const fileBase = exportFileBaseFromQuery({
      queryText: currentQueryText(),
      language,
      connectionName,
      collection: language === "mql" ? collection : null,
      database: language === "mql" ? database : null,
    });
    try {
      let res;
      switch (action) {
        case "download-csv":
          res = await exportRowsAsCsv(cols, rows, fileBase);
          break;
        case "download-json":
          res = await exportRowsAsJson(cols, rows, fileBase);
          break;
        case "download-excel":
          res = await exportRowsAsExcel(cols, rows, fileBase);
          break;
        case "copy-csv":
          res = await copyTextToClipboard(buildCsv(cols, rows));
          break;
        case "copy-json":
          res = await copyTextToClipboard(buildJson(cols, rows, true));
          break;
      }
      if (res.ok) {
        const labels: Record<ResultExportAction, string> = {
          "download-csv": "CSV downloaded",
          "download-json": "JSON downloaded",
          "download-excel": "Excel downloaded",
          "copy-csv": "CSV copied",
          "copy-json": "JSON copied",
        };
        exportMsg = labels[action];
      } else if (!res.cancelled) {
        exportMsg = res.error;
      }
    } catch (e) {
      exportMsg = e instanceof Error ? e.message : String(e);
    } finally {
      exporting = null;
      setTimeout(() => {
        if (
          exportMsg &&
          !exportMsg.toLowerCase().includes("fail") &&
          !exportMsg.toLowerCase().includes("error")
        ) {
          exportMsg = null;
        }
      }, 2500);
    }
  }

  function currentQueryText(): string {
    return localText || text || "";
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
      onhistory?.(null);
      return null;
    }
  }

  async function runQuery() {
    if (runState.status === "running") return;
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

    let sid = engineSessionId
    if (!sid && onreconnect) {
      sid = await onreconnect()
    }
    if (!sid) {
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
      const request = {
        sessionId: sid,
        text: sqlText,
        mode,
        maxRows: QUERY_FETCH_MAX_ROWS,
        timeoutMs: 120_000,
        profileReadOnly,
        collection: language === "mql" ? collection.trim() || null : null,
        database:
          language === "mql" && database?.trim() ? database.trim() : null,
      };
      let result
      try {
        result = await executeEngineQuery(request);
      } catch (e) {
        if (!onreconnect || !isSessionMissingError(e)) throw e
        const next = await onreconnect()
        if (!next) throw e
        result = await executeEngineQuery({ ...request, sessionId: next })
      }
      const elapsedMs = Math.round(performance.now() - start);
      runState = {
        status: "ok",
        message: (result.message ?? "").trim(),
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

  function clampSplit(pct: number): number {
    return Math.min(SPLIT_MAX, Math.max(SPLIT_MIN, Math.round(pct * 10) / 10));
  }

  function onSplitPointerDown(e: PointerEvent) {
    if (!splitRootEl) return;
    e.preventDefault();
    const root = splitRootEl;
    const startY = e.clientY;
    const startPct = editorSharePct;
    const rootH = root.getBoundingClientRect().height || 1;
    splitDragging = true;

    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);

    const onMove = (ev: PointerEvent) => {
      const deltaPct = ((ev.clientY - startY) / rootH) * 100;
      editorSharePct = clampSplit(startPct + deltaPct);
    };
    const onUp = (ev: PointerEvent) => {
      splitDragging = false;
      try {
        target.releasePointerCapture(ev.pointerId);
      } catch {
        /* ignore */
      }
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
  }

  function onSplitDblClick() {
    editorSharePct = SPLIT_DEFAULT;
  }

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return;
      if (e.key !== "Enter" && e.code !== "NumpadEnter") return;
      const t = e.target;
      if (t instanceof HTMLElement && t.closest('[role="dialog"]')) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      if (!currentQueryText().trim()) return;
      void runQuery();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });
</script>

<div
  class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash"
  data-connection-id={connectionId}
>
  <div
    bind:this={splitRootEl}
    class="flex min-h-0 flex-1 flex-col"
    class:select-none={splitDragging}
  >
    <div
      class="flex min-h-0 min-w-0 flex-col overflow-hidden"
      style="flex: 0 0 {editorSharePct}%; max-height: {SPLIT_MAX}%"
    >
      <QueryEditor
        {connectionId}
        {engine}
        {placeholder}
        bind:localText
        {lineCount}
        {language}
        {profileReadOnly}
        {effectiveMode}
        bind:collection
        {writeEnabled}
        canRun={!!localText.trim()}
        running={runState.status === "running"}
        {dirty}
        {saving}
        onchange={emitText}
        onwritechange={onToggleWrite}
        onrun={() => void runQuery()}
        onsave={onsave}
      />
    </div>

    <button
      type="button"
      aria-label="Resize editor and results"
      aria-valuemin={SPLIT_MIN}
      aria-valuemax={SPLIT_MAX}
      aria-valuenow={Math.round(editorSharePct)}
      class="group relative z-10 flex h-1.5 w-full shrink-0 cursor-row-resize items-center justify-center border-x-0 border-y border-line bg-elevated p-0 hover:bg-hover"
      class:bg-accent-muted={splitDragging}
      title="Drag to resize · double-click to reset"
      onpointerdown={onSplitPointerDown}
      ondblclick={onSplitDblClick}
      onkeydown={(e) => {
        if (e.key === "ArrowUp") {
          e.preventDefault();
          editorSharePct = clampSplit(editorSharePct - 2);
        } else if (e.key === "ArrowDown") {
          e.preventDefault();
          editorSharePct = clampSplit(editorSharePct + 2);
        } else if (e.key === "Home") {
          e.preventDefault();
          editorSharePct = SPLIT_DEFAULT;
        }
      }}
    >
      <span
        class="pointer-events-none h-0.5 w-10 rounded-full bg-line-strong opacity-70 group-hover:bg-accent group-hover:opacity-100"
        class:bg-accent={splitDragging}
        aria-hidden="true"
      ></span>
    </button>

    <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <QueryResultsPanel
        {runState}
        {resultColumns}
        {resultPageRows}
        {resultTotal}
        {resultPageStart}
        {resultPageEnd}
        {resultSafePage}
        {resultTotalPages}
        {canExport}
        {exporting}
        {exportMsg}
        refreshing={runState.status === "running"}
        onexport={(f) => void exportResults(f)}
        onprev={goResultPrev}
        onnext={goResultNext}
        onrefresh={() => void runQuery()}
      />
    </div>
  </div>
</div>
