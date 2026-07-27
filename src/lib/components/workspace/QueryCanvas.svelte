<script lang="ts">
  /**
   * Query space for a connected session.
   * Run + write-mode toggle; DELETE blocked (FE checks until BE policy SPI lands).
   */
  import { Button, Badge, Checkbox } from "$lib/components/ui";
  import { engineLabel } from "$lib/types/connection";
  import {
    queryLanguageForEngine,
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
    connectionName,
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

  const language = $derived(queryLanguageForEngine(engine));
  const placeholder = $derived(queryPlaceholder(engine));
  const effectiveMode = $derived<QueryMode>(
    profileReadOnly || !writeEnabled ? "read" : "write",
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
      </div>
      <p class="mt-0.5 text-caption text-fg-muted">
        {#if profileReadOnly}
          Connection is read-only · writes disabled
        {:else if effectiveMode === "read"}
          Read mode · enable writes for INSERT/UPDATE (DELETE blocked)
        {:else}
          Write mode · DELETE still blocked
        {/if}
      </p>
    </div>

    <div class="flex shrink-0 flex-wrap items-center gap-2">
      <label
        class={cn(
          "inline-flex items-center gap-2 rounded-md border px-2 py-1 text-label",
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

  <div class="flex min-h-0 flex-1 flex-col lg:flex-row">
    <div class="flex min-h-0 min-w-0 flex-1 flex-col border-b border-line lg:border-r lg:border-b-0">
      <label class="sr-only" for="query-editor-{connectionId}">Query editor</label>
      <textarea
        id="query-editor-{connectionId}"
        class={cn(
          "min-h-40 w-full flex-1 resize-none border-none bg-panel p-3",
          "font-mono text-body leading-relaxed text-fg",
          "placeholder:text-fg-muted focus-visible:outline-none",
        )}
        {placeholder}
        spellcheck="false"
        value={text}
        oninput={(e) => onTextInput((e.currentTarget as HTMLTextAreaElement).value)}
        onkeydown={onKeydown}
      ></textarea>
      <div
        class="flex h-statusbar shrink-0 items-center justify-between gap-2 border-t border-line bg-elevated px-2.5 text-caption text-fg-muted"
      >
        <span>
          ⌘/Ctrl+Enter to run
          {#if effectiveMode === "write"}
            · <span class="text-warning">writes on</span>
          {/if}
        </span>
        <span class="font-mono tabular-nums">{text.length} chars</span>
      </div>
    </div>

    <div class="flex min-h-32 min-w-0 flex-1 flex-col bg-app-wash lg:max-w-md xl:max-w-lg">
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
