<script lang="ts">
  import { Button } from "$lib/components/ui";
  import {
    formatRelativeTime,
    type QueryHistoryEntry,
    type SavedQuery,
  } from "$lib/data/mock-queries";

  interface Props {
    kind: "saved" | "history";
    saved?: SavedQuery | null;
    history?: QueryHistoryEntry | null;
  }

  let { kind, saved = null, history = null }: Props = $props();

  const title = $derived(
    kind === "saved" ? (saved?.name ?? "Saved query") : "Query history",
  );
  const sql = $derived(
    kind === "saved" ? (saved?.sql ?? "") : (history?.sql ?? ""),
  );
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
  <div
    class="flex shrink-0 items-center justify-between gap-3 border-b border-line bg-elevated px-3 py-2"
  >
    <div class="min-w-0">
      <div class="truncate text-label font-semibold text-fg">{title}</div>
      <div class="truncate text-caption text-fg-muted">
        {#if kind === "saved" && saved}
          {#if saved.folder}
            {saved.folder}
            <span aria-hidden="true"> · </span>
          {/if}
          Updated {formatRelativeTime(saved.updatedAt)}
        {:else if history}
          {formatRelativeTime(history.ranAt)}
          <span aria-hidden="true"> · </span>
          <span class="font-mono tabular-nums">{history.durationMs}ms</span>
          {#if history.status === "success"}
            <span aria-hidden="true"> · </span>
            <span class="text-success">success</span>
            {#if history.rowCount != null}
              <span aria-hidden="true"> · </span>
              <span class="font-mono tabular-nums">{history.rowCount} rows</span>
            {/if}
          {:else}
            <span aria-hidden="true"> · </span>
            <span class="text-danger">error</span>
          {/if}
        {/if}
      </div>
    </div>
    <div class="flex shrink-0 gap-1.5">
      <Button variant="secondary" size="sm" disabled>Open in editor</Button>
      <Button variant="primary" size="sm" disabled>Run</Button>
    </div>
  </div>

  <div class="min-h-0 flex-1 overflow-auto p-3">
    <pre
      class="m-0 rounded-md border border-line bg-panel p-3 font-mono text-label leading-relaxed text-fg whitespace-pre-wrap"
    >{sql}</pre>
    {#if kind === "history" && history?.error}
      <div
        class="mt-3 rounded-md border border-danger/40 bg-danger/15 px-3 py-2 text-label text-danger"
      >
        {history.error}
      </div>
    {/if}
  </div>

  <footer
    class="flex h-statusbar shrink-0 items-center border-t border-line bg-elevated px-2.5 text-caption text-fg-muted"
  >
    Dummy data · Run / editor wiring later
  </footer>
</div>
