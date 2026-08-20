<script lang="ts">
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils/cn";

  type Tone = "info" | "success" | "warning" | "danger";

  interface Props {
    tone?: Tone;
    children?: Snippet;
  }

  let { tone = "info", children }: Props = $props();

  const tones: Record<Tone, string> = {
    info: "border-info/35 bg-info/12 text-fg",
    success: "border-success/35 bg-success/15 text-success",
    warning: "border-warning/35 bg-warning/15 text-warning",
    danger: "border-danger/35 bg-danger/15 text-danger",
  };
</script>

<div
  class={cn("m-0 rounded-md border px-3 py-2.5 text-caption", tones[tone])}
  role="alert"
>
  {@render children?.()}
</div>
