<script lang="ts">
  import AppSettings from "./AppSettings.svelte";
  import IconButton from "./IconButton.svelte";
  import { cn } from "$lib/utils/cn";

  interface Props {
    /** Panel position relative to the gear. */
    placement?: "top-end" | "right-end";
    buttonSize?: "sm" | "md";
  }

  let { placement = "top-end", buttonSize = "sm" }: Props = $props();

  let open = $state(false);
  let wrap: HTMLDivElement | undefined = $state();

  function toggle() {
    open = !open;
  }

  function onWindowPointerDown(e: PointerEvent) {
    if (!open) return;
    const t = e.target;
    if (t instanceof Node && wrap?.contains(t)) return;
    open = false;
  }

  function onWindowKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && open) open = false;
  }
</script>

<div bind:this={wrap} class="relative">
  <IconButton
    label="Settings"
    variant="ghost"
    size={buttonSize}
    active={open}
    onclick={toggle}
    aria-expanded={open}
    aria-haspopup="dialog"
  >
    <svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M6.6 1.85h2.8l.3 1.45c.42.16.81.38 1.16.66l1.48-.38 1.4 2.42-1.18.9c.07.3.11.62.11 1.1 0 .37-.04.69-.11 1.01l1.18.9-1.4 2.42-1.48-.38a4.6 4.6 0 0 1-1.16.66l-.3 1.45H6.6l-.3-1.45a4.6 4.6 0 0 1-1.16-.66l-1.48.38-1.4-2.42 1.18-.9A4.4 4.4 0 0 1 3.33 8c0-.37.04-.69.11-1.01l-1.18-.9 1.4-2.42 1.48.38c.35-.28.74-.5 1.16-.66l.3-1.45Z"
        stroke="currentColor"
        stroke-width="1.2"
        stroke-linejoin="round"
      />
      <circle cx="8" cy="8" r="1.65" stroke="currentColor" stroke-width="1.2" />
    </svg>
  </IconButton>
  {#if open}
    <div
      class={cn(
        "absolute z-40 w-72 rounded-lg border border-line bg-elevated p-3 shadow-md",
        placement === "top-end" && "right-0 bottom-full mb-2",
        placement === "right-end" && "bottom-0 left-full ml-2",
      )}
      role="dialog"
      aria-label="App settings"
    >
      <p
        class="m-0 mb-3 text-label font-semibold tracking-wider text-fg-secondary uppercase"
      >
        Settings
      </p>
      <AppSettings />
    </div>
  {/if}
</div>

<svelte:window onpointerdown={onWindowPointerDown} onkeydown={onWindowKeydown} />
