<script lang="ts">
  import Button from "./Button.svelte";

  interface Props {
    /** 0-based page index. */
    page: number;
    totalPages: number;
    disabled?: boolean;
    /** When false, show an em dash instead of page numbers. */
    active?: boolean;
    onprev: () => void;
    onnext: () => void;
    ariaLabel?: string;
  }

  let {
    page,
    totalPages,
    disabled = false,
    active = true,
    onprev,
    onnext,
    ariaLabel = "Pagination",
  }: Props = $props();

  const label = $derived(
    active && totalPages > 0 ? `${page + 1}/${totalPages}` : "—",
  );
</script>

<div class="flex items-center gap-0.5" role="group" aria-label={ariaLabel}>
  <Button
    variant="ghost"
    size="sm"
    disabled={disabled || !active || page <= 0}
    onclick={onprev}
    aria-label="Previous page"
  >
    ‹
  </Button>
  <span
    class="min-w-12 text-center font-mono text-caption text-fg-secondary tabular-nums"
  >
    {label}
  </span>
  <Button
    variant="ghost"
    size="sm"
    disabled={disabled || !active || page >= totalPages - 1}
    onclick={onnext}
    aria-label="Next page"
  >
    ›
  </Button>
</div>
