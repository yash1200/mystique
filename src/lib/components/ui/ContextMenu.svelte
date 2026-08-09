<script lang="ts">
  import { onMount } from "svelte";
  import { cn } from "$lib/utils/cn";

  export type ContextMenuItem = {
    id: string;
    label: string;
    danger?: boolean;
    disabled?: boolean;
    separatorBefore?: boolean;
  };

  interface Props {
    x: number;
    y: number;
    items: ContextMenuItem[];
    ariaLabel?: string;
    onpick: (id: string) => void;
    onclose: () => void;
  }

  let { x, y, items, ariaLabel = "Actions", onpick, onclose }: Props = $props();

  let root = $state<HTMLDivElement | null>(null);
  let left = $state(0);
  let top = $state(0);

  onMount(() => {
    left = x;
    top = y;
    const el = root;
    if (el) {
      const rect = el.getBoundingClientRect();
      const pad = 8;
      left = Math.max(pad, Math.min(x, window.innerWidth - rect.width - pad));
      top = Math.max(pad, Math.min(y, window.innerHeight - rect.height - pad));
    }
    const onDoc = (e: PointerEvent) => {
      if (e.target instanceof Node && root?.contains(e.target)) return;
      onclose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onclose();
    };
    // Wait a tick so the opening right-click cannot close the menu.
    const tid = window.setTimeout(() => {
      document.addEventListener("pointerdown", onDoc, true);
    }, 0);
    window.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(tid);
      document.removeEventListener("pointerdown", onDoc, true);
      window.removeEventListener("keydown", onKey);
    };
  });

  function pick(id: string, disabled?: boolean) {
    if (disabled) return;
    onpick(id);
  }
</script>

<div
  bind:this={root}
  class="fixed z-50 min-w-44 rounded-md border border-line bg-elevated py-1 shadow-md"
  style="left:{left}px; top:{top}px"
  role="menu"
  tabindex="-1"
  aria-label={ariaLabel}
  onpointerdown={(e) => e.stopPropagation()}
  oncontextmenu={(e) => e.preventDefault()}
>
  {#each items as item (item.id)}
    {#if item.separatorBefore}
      <div class="my-1 border-t border-line" role="separator"></div>
    {/if}
    <button
      type="button"
      role="menuitem"
      disabled={item.disabled}
      class={cn(
        "flex w-full cursor-pointer border-none bg-transparent px-2.5 py-1.5 text-left text-label",
        item.danger
          ? "text-danger hover:bg-danger/10"
          : "text-fg hover:bg-hover",
        item.disabled && "cursor-not-allowed opacity-45 hover:bg-transparent",
      )}
      onpointerdown={(e) => {
        e.stopPropagation();
        pick(item.id, item.disabled);
      }}
    >
      {item.label}
    </button>
  {/each}
</div>
