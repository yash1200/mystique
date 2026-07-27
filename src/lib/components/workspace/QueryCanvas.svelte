<script lang="ts">
  /**
   * Query space for a connected session.
   * Run + write-mode toggle; DELETE blocked (FE checks until BE policy SPI lands).
   */
  import { Button, Badge, Checkbox } from "$lib/components/ui";
  import {
    queryPlaceholder,
    validateQueryFe,
    type QueryMode,
  } from "$lib/utils/queryPolicyFe";
  import { cn } from "$lib/utils/cn";

  interface Props {
    connectionId: string;
    connectionName: string;
    engine: string;
    /** Profile-level force read-only. */
    profileReadOnly: boolean;
    text?: string;
    writeEnabled?: boolean;
    ontextchange?: (text: string) => void;
    onwritechange?: (enabled: boolean) => void;
  }

  let {
    connectionId,
    engine,
    profileReadOnly,
    text = "",
    writeEnabled = false,
    ontextchange,
    onwritechange,
  }: Props = $props();

  type RunState =
    | { status: "idle" }
    | { status: "running" }
    | {
        status: "ok" | "blocked" | "error";
        message: string;
        mode: QueryMode;
        elapsedMs: number;
      };

  let runState = $state<RunState>({ status: "idle" });
  let editorEl = $state<HTMLTextAreaElement | null>(null);
  let gutterEl = $state<HTMLDivElement | null>(null);
  let splitEl = $state<HTMLDivElement | null>(null);

  /** Results pane height as % of the editor+results stack (rest is editor). */
  let resultsPct = $state(40);
  let draggingSplit = $state(false);

  const RESULTS_PCT_MIN = 15;
  const RESULTS_PCT_MAX = 80;
  const SPLIT_STORAGE_PREFIX = "mystique-query-results-pct:";

  const placeholder = $derived(queryPlaceholder(engine));
  const effectiveMode = $derived<QueryMode>(
    profileReadOnly || !writeEnabled ? "read" : "write",
  );

  // Restore per-connection split preference
  $effect(() => {
    void connectionId;
    try {
      const raw = localStorage.getItem(SPLIT_STORAGE_PREFIX + connectionId);
      const n = raw != null ? Number(raw) : NaN;
      if (Number.isFinite(n)) {
        resultsPct = Math.min(RESULTS_PCT_MAX, Math.max(RESULTS_PCT_MIN, n));
      } else {
        resultsPct = 40;
      }
    } catch {
      resultsPct = 40;
    }
  });

  /** Line count for the gutter (at least 1, even when empty). */
  const lineCount = $derived.by(() => {
    if (!text) return 1;
    // Trailing newline still owns an empty last line in most editors
    return text.split("\n").length;
  });

  const lineNumbers = $derived(
    Array.from({ length: lineCount }, (_, i) => i + 1),
  );

  const gutterWidthCh = $derived(
    Math.max(2, String(lineCount).length) + 1,
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

  function onTextInput(value: string) {
    ontextchange?.(value);
  }

  async function runQuery() {
    const mode = effectiveMode;
    const start = performance.now();
    runState = { status: "running" };

    await new Promise((r) => setTimeout(r, 120));

    const check = validateQueryFe(engine, text, mode);
    const elapsedMs = Math.round(performance.now() - start);

    if (!check.ok) {
      runState = {
        status: "blocked",
        message: check.message,
        mode,
        elapsedMs,
      };
      return;
    }

    runState = {
      status: "ok",
      message: `${check.summary} Backend execute is not wired yet — statement was validated in the app only.`,
      mode,
      elapsedMs,
    };
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      void runQuery();
    }
  }

  function syncGutterScroll() {
    if (editorEl && gutterEl) {
      gutterEl.scrollTop = editorEl.scrollTop;
    }
  }

  function onEditorScroll() {
    syncGutterScroll();
  }

  function clampResultsPct(n: number): number {
    return Math.min(RESULTS_PCT_MAX, Math.max(RESULTS_PCT_MIN, n));
  }

  function persistResultsPct(pct: number) {
    try {
      localStorage.setItem(SPLIT_STORAGE_PREFIX + connectionId, String(pct));
    } catch {
      /* ignore */
    }
  }

  function onSplitPointerDown(e: PointerEvent) {
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    draggingSplit = true;
  }

  function onSplitPointerMove(e: PointerEvent) {
    if (!draggingSplit || !splitEl) return;
    const rect = splitEl.getBoundingClientRect();
    if (rect.height <= 0) return;
    // Distance from bottom of stack → results height %
    const fromBottom = rect.bottom - e.clientY;
    const pct = clampResultsPct((fromBottom / rect.height) * 100);
    resultsPct = pct;
  }

  function onSplitPointerUp(e: PointerEvent) {
    if (!draggingSplit) return;
    draggingSplit = false;
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
    persistResultsPct(resultsPct);
  }
</script>

<div
  class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash"
  data-connection-id={connectionId}
>
  <!-- Column layout: editor on top, drag handle, results below (resizable) -->
  <div bind:this={splitEl} class="flex min-h-0 flex-1 flex-col">
    <div
      class="flex min-h-0 min-w-0 flex-col"
      style="flex: 1 1 {100 - resultsPct}%; min-height: 5rem"
    >
      <label class="sr-only" for="query-editor-{connectionId}">Query editor</label>
      <div class="flex min-h-0 min-w-0 flex-1 bg-panel">
        <!-- Line numbers gutter (scroll-synced with the textarea) -->
        <div
          bind:this={gutterEl}
          class={cn(
            "shrink-0 overflow-hidden border-r border-line bg-elevated select-none",
            "font-mono text-body leading-relaxed text-fg-muted",
          )}
          style="width: calc({gutterWidthCh}ch + 1.25rem)"
          aria-hidden="true"
        >
          <div class="box-border py-3 pr-2 pl-2 text-right tabular-nums leading-relaxed">
            {#each lineNumbers as n (n)}
              <div>{n}</div>
            {/each}
          </div>
        </div>
        <textarea
          bind:this={editorEl}
          id="query-editor-{connectionId}"
          class={cn(
            "min-h-0 w-full min-w-0 flex-1 resize-none border-none bg-panel py-3 pr-3 pl-3",
            "font-mono text-body leading-relaxed text-fg",
            "placeholder:text-fg-muted focus-visible:outline-none",
          )}
          {placeholder}
          spellcheck="false"
          wrap="off"
          value={text}
          oninput={(e) =>
            onTextInput((e.currentTarget as HTMLTextAreaElement).value)}
          onkeydown={onKeydown}
          onscroll={onEditorScroll}
        ></textarea>
      </div>
      <!-- Toolbar just above the results resize handle -->
      <div
        class="flex h-statusbar shrink-0 items-center justify-between gap-2 border-t border-line bg-elevated px-2.5"
      >
        <div class="flex min-w-0 items-center gap-2 text-caption text-fg-muted">
          <span class="truncate">
            ⌘/Ctrl+Enter to run
            {#if effectiveMode === "write"}
              · <span class="text-warning">writes on</span>
            {/if}
          </span>
          <span class="shrink-0 font-mono tabular-nums">
            {lineCount} line{lineCount === 1 ? "" : "s"}
            · {text.length} chars
          </span>
        </div>
        <div class="flex shrink-0 items-center gap-1.5">
          <label
            class={cn(
              "inline-flex h-control-sm items-center gap-1.5 rounded-md border px-2 text-label",
              profileReadOnly
                ? "cursor-not-allowed border-line bg-input text-fg-muted opacity-70"
                : effectiveMode === "write"
                  ? "border-warning/50 bg-warning/10 text-fg"
                  : "border-line bg-input text-fg-secondary",
            )}
            title={profileReadOnly
              ? "This connection profile is read-only"
              : "Allow write statements (DELETE remains blocked)"}
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
    </div>

    <!-- Drag to resize results height -->
    <div
      class={cn(
        "group relative z-10 flex h-2 shrink-0 cursor-row-resize items-center justify-center",
        "border-y border-line bg-elevated",
        "hover:bg-hover",
        draggingSplit && "bg-accent-muted",
      )}
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize results panel"
      aria-valuemin={RESULTS_PCT_MIN}
      aria-valuemax={RESULTS_PCT_MAX}
      aria-valuenow={Math.round(resultsPct)}
      tabindex="0"
      onpointerdown={onSplitPointerDown}
      onpointermove={onSplitPointerMove}
      onpointerup={onSplitPointerUp}
      onpointercancel={onSplitPointerUp}
      onkeydown={(e) => {
        if (e.key === "ArrowUp") {
          e.preventDefault();
          resultsPct = clampResultsPct(resultsPct + 2);
          persistResultsPct(resultsPct);
        } else if (e.key === "ArrowDown") {
          e.preventDefault();
          resultsPct = clampResultsPct(resultsPct - 2);
          persistResultsPct(resultsPct);
        }
      }}
    >
      <span
        class={cn(
          "h-0.5 w-10 rounded-full bg-line-strong transition-colors",
          "group-hover:bg-accent",
          draggingSplit && "bg-accent",
        )}
        aria-hidden="true"
      ></span>
    </div>

    <div
      class="flex min-h-0 min-w-0 flex-col bg-app-wash"
      style="flex: 1 1 {resultsPct}%; min-height: 5rem"
    >
      <div
        class="flex h-control-sm shrink-0 items-center border-b border-line bg-elevated px-3 text-label font-medium text-fg-secondary"
      >
        Results
      </div>
      <div class="min-h-0 flex-1 overflow-auto p-3">
        {#if runState.status === "idle"}
          <p class="text-body text-fg-muted">
            Write a query and press <strong class="font-medium text-fg-secondary">Run</strong>.
            Validation runs in the app; live engine execute comes with the backend policy SPI.
          </p>
        {:else if runState.status === "running"}
          <p class="text-body text-fg-muted">Validating…</p>
        {:else}
          <div
            class={cn(
              "rounded-md border px-3 py-2 text-label",
              runState.status === "ok" && "border-success/40 bg-success/10 text-fg",
              runState.status === "blocked" && "border-danger/40 bg-danger/10 text-danger",
              runState.status === "error" && "border-danger/40 bg-danger/10 text-danger",
            )}
            role="status"
          >
            <div class="mb-1 flex flex-wrap items-center gap-2">
              <Badge tone={runState.status === "ok" ? "success" : "danger"}>
                {runState.status === "ok" ? "OK" : "Blocked"}
              </Badge>
              <span class="text-caption text-fg-muted">
                mode={runState.mode}
                · {runState.elapsedMs}ms
              </span>
            </div>
            <p class="m-0 whitespace-pre-wrap">{runState.message}</p>
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>
