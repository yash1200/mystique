<script lang="ts">
  import { cn } from "$lib/utils/cn";

  interface Props {
    size?: "sm" | "md";
    class?: string;
  }

  let { size = "md", class: className = "" }: Props = $props();
</script>

<span
  class={cn(
    "grid shrink-0 place-items-center rounded-lg bg-accent text-fg-inverse shadow-sm",
    size === "sm" ? "size-7" : "size-8",
    className,
  )}
  aria-hidden="true"
>
  <svg class={size === "sm" ? "size-3.5" : "size-4"} viewBox="0 0 16 16" fill="none">
    <path
      d="M2.2 8c2.4-3.6 9.2-3.6 11.6 0-2.4 3.6-9.2 3.6-11.6 0Z"
      stroke="currentColor"
      stroke-width="1.35"
      stroke-linejoin="round"
    />
    <circle cx="8" cy="8" r="1.85" fill="currentColor" />
  </svg>
</span>
