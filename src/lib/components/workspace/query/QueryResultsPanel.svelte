<script lang="ts">
  import { onMount } from "svelte";
  import { Button, Select } from "$lib/components/ui";
  import type { QueryResult } from "$lib/api/engines";
  import { cn } from "$lib/utils/cn";
  import type { RunState } from "./types";
  import QueryResultsGrid from "./QueryResultsGrid.svelte";

  export type ResultExportAction =
    | "download-csv"
    | "download-json"
    | "download-excel"
    | "copy-csv"
    | "copy-json";

  interface Props {
    runState: RunState;
    resultColumns: QueryResult["columns"];
    resultPageRows: unknown[][];
    resultTotal: number;
    resultPageStart: number;
    resultPageEnd: number;
    resultSafePage: number;
    resultTotalPages: number;
    pageSize: number;
    pageSizeOptions: { value: string; label: string }[];
    canExport: boolean;
    exporting: ResultExportAction | null;
    exportMsg: string | null;
    refreshing: boolean;
    onexport: (action: ResultExportAction) => void;
    onprev: () => void;
    onnext: () => void;
    onSetPageSize: (v: string) => void;
    onrefresh: () => void;
  }

  let {
    runState,
    resultColumns,
    resultPageRows,
    resultTotal,
    resultPageStart,
    resultPageEnd,
    resultSafePage,
    resultTotalPages,
    pageSize,
    pageSizeOptions,
    canExport,
    exporting,
    exportMsg,
    refreshing,
    onexport,
    onprev,
    onnext,
    onSetPageSize,
    onrefresh,
  }: Props = $props();

  let menuOpen = $state(false);
  let menuRoot = $state<HTMLDivElement | null>(null);
  let downloadBtnEl = $state<HTMLElement | null>(null);
  let menuStyle = $state("");

  const statusText = $derived.by(() => {
    if (runState.status === "idle") return "No query run yet";
    if (runState.status === "running") return "Running…";
    if (
      runState.status === "ok" ||
      runState.status === "blocked" ||
      runState.status === "error"
    ) {
      return runState.message;
    }
    return "";
  });

  const statusTone = $derived(
    runState.status === "ok"
      ? "text-success"
      : runState.status === "blocked" || runState.status === "error"
        ? "text-danger"
        : "text-fg-muted",
  );

  const hasRows = $derived(
    runState.status === "ok" && resultTotal > 0 && resultColumns.length > 0,
  );

  function updateMenuPosition() {
    const anchor = downloadBtnEl ?? menuRoot;
    if (!anchor) return;
    const rect = anchor.getBoundingClientRect();
    const gap = 4;
    // Status bar sits at the bottom — open upward with fixed coords so
    // footer overflow / stacking contexts cannot clip the menu.
    const right = Math.max(8, Math.round(window.innerWidth - rect.right));
    const bottom = Math.max(8, Math.round(window.innerHeight - rect.top + gap));
    menuStyle = [
      "position:fixed",
      `right:${right}px`,
      `bottom:${bottom}px`,
      "z-index:80",
      "min-width:11rem",
    ].join(";");
  }

  function toggleMenu(e?: MouseEvent) {
    if (!canExport || exporting !== null || refreshing) return;
    if (menuOpen) {
      menuOpen = false;
      return;
    }
    const t = e?.currentTarget;
    if (t instanceof HTMLElement) downloadBtnEl = t;
    updateMenuPosition();
    menuOpen = true;
  }

  function pick(action: ResultExportAction) {
    menuOpen = false;
    onexport(action);
  }

  onMount(() => {
    const onDoc = (e: PointerEvent) => {
      if (!menuOpen || !menuRoot) return;
      if (e.target instanceof Node && menuRoot.contains(e.target)) return;
      menuOpen = false;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") menuOpen = false;
    };
    const onReposition = () => {
      if (menuOpen) updateMenuPosition();
    };
    document.addEventListener("pointerdown", onDoc, true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onReposition);
    window.addEventListener("scroll", onReposition, true);
    return () => {
      document.removeEventListener("pointerdown", onDoc, true);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onReposition);
      window.removeEventListener("scroll", onReposition, true);
    };
  });
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
  <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
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
        <QueryResultsGrid
          columns={resultColumns}
          rows={resultPageRows}
          pageStart={resultPageStart}
        />
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

  <!-- Match table GridStatusBar: left rows · center pages · right actions -->
  <footer
    class="relative z-20 flex h-statusbar min-h-statusbar shrink-0 items-center gap-2 border-t border-line bg-elevated px-2.5"
  >
    <!-- Left: row / status info -->
    <div
      class="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden text-caption text-fg-muted"
    >
      {#if hasRows}
        <span class="truncate font-mono tabular-nums">
          {resultPageStart + 1}–{resultPageEnd} of {resultTotal.toLocaleString()}
        </span>
      {:else}
        <span class="truncate font-mono tabular-nums">
          {runState.status === "ok" && resultColumns.length > 0
            ? "0 rows"
            : "—"}
        </span>
      {/if}
      {#if statusText}
        <span class={cn("min-w-0 truncate", statusTone)} title={statusText}>
          · {statusText}
        </span>
      {/if}
      {#if exportMsg}
        <span class="shrink-0 truncate text-success" title={exportMsg}
          >· {exportMsg}</span
        >
      {/if}
    </div>

    <!-- Center: pagination -->
    <div
      class="flex shrink-0 items-center gap-1.5"
      role="group"
      aria-label="Pagination"
    >
      <div class="w-28 shrink-0">
        <Select
          id="query-result-page-size"
          value={String(pageSize)}
          options={pageSizeOptions}
          inputSize="sm"
          onchange={onSetPageSize}
          disabled={!hasRows}
        />
      </div>
      <div class="flex items-center gap-0.5">
        <Button
          variant="ghost"
          size="sm"
          disabled={!hasRows || resultSafePage <= 0}
          onclick={onprev}
          aria-label="Previous page"
        >
          ‹
        </Button>
        <span
          class="min-w-12 text-center font-mono text-caption text-fg-secondary tabular-nums"
        >
          {hasRows ? `${resultSafePage + 1}/${resultTotalPages}` : "—"}
        </span>
        <Button
          variant="ghost"
          size="sm"
          disabled={!hasRows || resultSafePage >= resultTotalPages - 1}
          onclick={onnext}
          aria-label="Next page"
        >
          ›
        </Button>
      </div>
    </div>

    <!-- Right: download + refresh -->
    <div class="flex min-w-0 flex-1 items-center justify-end gap-1">
      <div class="relative" bind:this={menuRoot}>
        <Button
          variant="ghost"
          size="sm"
          disabled={!canExport || exporting !== null || refreshing}
          onclick={(e) => toggleMenu(e)}
          title="Download or copy results"
          aria-haspopup="menu"
          aria-expanded={menuOpen}
        >
          {exporting ? "…" : "Download"}
        </Button>
        {#if menuOpen && canExport}
          <div
            class="rounded-md border border-line bg-elevated py-1 shadow-md"
            style={menuStyle}
            role="menu"
          >
            <p
              class="px-2.5 py-1 text-micro font-semibold tracking-wide text-fg-muted uppercase"
            >
              Download
            </p>
            <button
              type="button"
              role="menuitem"
              class="flex w-full cursor-pointer border-none bg-transparent px-2.5 py-1.5 text-left text-label text-fg hover:bg-hover"
              onclick={() => pick("download-csv")}
            >
              CSV
            </button>
            <button
              type="button"
              role="menuitem"
              class="flex w-full cursor-pointer border-none bg-transparent px-2.5 py-1.5 text-left text-label text-fg hover:bg-hover"
              onclick={() => pick("download-json")}
            >
              JSON
            </button>
            <button
              type="button"
              role="menuitem"
              class="flex w-full cursor-pointer border-none bg-transparent px-2.5 py-1.5 text-left text-label text-fg hover:bg-hover"
              onclick={() => pick("download-excel")}
            >
              Excel
            </button>
            <div class="my-1 border-t border-line"></div>
            <p
              class="px-2.5 py-1 text-micro font-semibold tracking-wide text-fg-muted uppercase"
            >
              Copy
            </p>
            <button
              type="button"
              role="menuitem"
              class="flex w-full cursor-pointer border-none bg-transparent px-2.5 py-1.5 text-left text-label text-fg hover:bg-hover"
              onclick={() => pick("copy-csv")}
            >
              Copy as CSV
            </button>
            <button
              type="button"
              role="menuitem"
              class="flex w-full cursor-pointer border-none bg-transparent px-2.5 py-1.5 text-left text-label text-fg hover:bg-hover"
              onclick={() => pick("copy-json")}
            >
              Copy as JSON
            </button>
          </div>
        {/if}
      </div>

      <Button
        variant="ghost"
        size="sm"
        disabled={refreshing || runState.status === "idle"}
        onclick={onrefresh}
        title="Re-run query"
      >
        {refreshing ? "Refreshing…" : "Refresh"}
      </Button>
    </div>
  </footer>
</div>
