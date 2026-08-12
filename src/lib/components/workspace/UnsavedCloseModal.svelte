<script lang="ts">
  import { Button } from "$lib/components/ui";

  interface Props {
    open: boolean;
    tabTitle: string;
    tabKind: "query" | "table" | "model" | "structure";
    busy?: boolean;
    onsave: () => void;
    ondiscard: () => void;
    oncancel: () => void;
  }

  let {
    open,
    tabTitle,
    tabKind,
    busy = false,
    onsave,
    ondiscard,
    oncancel,
  }: Props = $props();

  const kindLabel = $derived(
    tabKind === "query"
      ? "query"
      : tabKind === "table"
        ? "table"
        : tabKind === "structure"
          ? "structure"
          : "tab",
  );
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget && !busy) oncancel();
    }}
    onkeydown={(e) => {
      if (e.key === "Escape" && !busy) oncancel();
    }}
  >
    <div
      class="w-full max-w-md rounded-lg border border-line bg-elevated p-4 shadow-lg"
      role="dialog"
      aria-modal="true"
      aria-labelledby="unsaved-close-title"
    >
      <h2
        id="unsaved-close-title"
        class="m-0 text-body font-semibold text-fg"
      >
        Unsaved changes
      </h2>
      <p class="mt-2 mb-0 text-label leading-relaxed text-fg-secondary">
        “{tabTitle}” has unsaved {kindLabel} changes. Save them before closing,
        discard them, or keep the tab open.
      </p>

      <div class="mt-4 flex flex-wrap items-center justify-end gap-2">
        <Button
          variant="ghost"
          size="sm"
          disabled={busy}
          onclick={oncancel}
        >
          Cancel
        </Button>
        <Button
          variant="ghost"
          size="sm"
          disabled={busy}
          onclick={ondiscard}
        >
          Discard
        </Button>
        <Button
          variant="primary"
          size="sm"
          disabled={busy}
          onclick={onsave}
        >
          {busy ? "Saving…" : "Save"}
        </Button>
      </div>
    </div>
  </div>
{/if}
