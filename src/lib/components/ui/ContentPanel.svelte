<script lang="ts">
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils/cn";

  type PanelSize = "sm" | "md" | "lg" | "full";

  interface Props {
    /** Prefer size tokens over raw maxWidth numbers. */
    size?: PanelSize;
    maxWidth?: number | string;
    ariaLabel?: string;
    class?: string;
    children?: Snippet;
  }

  let {
    size = "md",
    maxWidth,
    ariaLabel,
    class: className = "",
    children,
  }: Props = $props();

  const sizeClass: Record<PanelSize, string> = {
    sm: "max-w-panel-sm",
    md: "max-w-panel-md",
    lg: "max-w-panel-lg",
    full: "max-w-none",
  };

  const maxStyle = $derived(
    maxWidth == null
      ? undefined
      : typeof maxWidth === "number"
        ? `${maxWidth}px`
        : maxWidth,
  );
</script>

<section
  class={cn(
    "mx-auto w-full px-6 py-8",
    !maxWidth && sizeClass[size],
    className,
  )}
  style={maxStyle ? `max-width: ${maxStyle}` : undefined}
  aria-label={ariaLabel}
>
  {@render children?.()}
</section>
