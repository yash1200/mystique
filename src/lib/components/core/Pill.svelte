<script lang="ts">
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils/cn";

  type Tone = "neutral" | "warning" | "success" | "danger" | "info";

  interface Props {
    tone?: Tone;
    uppercase?: boolean;
    children?: Snippet;
  }

  let { tone = "neutral", uppercase = false, children }: Props = $props();

  const tones: Record<Tone, string> = {
    neutral: "border-line text-fg-secondary",
    warning: "border-warning/45 bg-warning/15 text-warning",
    success: "border-success/40 bg-success/15 text-success",
    danger: "border-danger/40 bg-danger/15 text-danger",
    info: "border-info/40 bg-info/15 text-info",
  };
</script>

<span
  class={cn(
    "inline-flex shrink-0 items-center border font-semibold leading-tight",
    uppercase
      ? "rounded-sm px-1 py-px text-micro uppercase tracking-wider"
      : "rounded-full px-2 py-0.5 text-caption",
    tones[tone],
  )}
>
  {@render children?.()}
</span>
