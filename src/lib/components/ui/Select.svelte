<script lang="ts">
  /**
   * Custom select — same look on every OS (no native <select> menu).
   * Menu is position:fixed so it isn’t clipped by overflow parents.
   */
  import { cn } from "$lib/utils/cn";

  export type SelectOption = {
    value: string;
    label: string;
    disabled?: boolean;
  };

  interface Props {
    value?: string;
    options?: SelectOption[];
    placeholder?: string;
    disabled?: boolean;
    inputSize?: "sm" | "md";
    class?: string;
    id?: string;
    /** Fires after the bound value changes. */
    onchange?: (value: string) => void;
  }

  let {
    value = $bindable(""),
    options = [],
    placeholder = "Select…",
    disabled = false,
    inputSize = "md",
    class: className = "",
    id,
    onchange,
  }: Props = $props();

  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let listEl = $state<HTMLUListElement | null>(null);
  let highlighted = $state(-1);
  let menuStyle = $state("");

  const listId = $derived(id ? `${id}-listbox` : undefined);
  const selected = $derived(options.find((o) => o.value === value));
  const displayLabel = $derived(selected?.label ?? (value ? String(value) : placeholder));
  const showingPlaceholder = $derived(!selected && !value);

  function firstEnabledIndex(): number {
    return options.findIndex((o) => !o.disabled);
  }

  function lastEnabledIndex(): number {
    for (let i = options.length - 1; i >= 0; i--) {
      if (!options[i]?.disabled) return i;
    }
    return -1;
  }

  function updateMenuPosition() {
    if (!triggerEl) return;
    const rect = triggerEl.getBoundingClientRect();
    const maxH = 240;
    const gap = 4;
    const spaceBelow = window.innerHeight - rect.bottom - 8;
    const spaceAbove = rect.top - 8;
    const openUp = spaceBelow < 140 && spaceAbove > spaceBelow;
    const height = Math.min(maxH, Math.max(80, openUp ? spaceAbove : spaceBelow));

    if (openUp) {
      menuStyle = [
        "position:fixed",
        `left:${Math.round(rect.left)}px`,
        `width:${Math.round(rect.width)}px`,
        `bottom:${Math.round(window.innerHeight - rect.top + gap)}px`,
        `max-height:${Math.round(height)}px`,
      ].join(";");
    } else {
      menuStyle = [
        "position:fixed",
        `left:${Math.round(rect.left)}px`,
        `width:${Math.round(rect.width)}px`,
        `top:${Math.round(rect.bottom + gap)}px`,
        `max-height:${Math.round(height)}px`,
      ].join(";");
    }
  }

  function openMenu() {
    if (disabled || options.length === 0) return;
    open = true;
    const current = options.findIndex((o) => o.value === value && !o.disabled);
    highlighted = current >= 0 ? current : firstEnabledIndex();
    requestAnimationFrame(() => {
      updateMenuPosition();
      listEl?.focus();
      scrollHighlightedIntoView();
    });
  }

  function closeMenu(focusTrigger = true) {
    if (!open) return;
    open = false;
    highlighted = -1;
    if (focusTrigger) triggerEl?.focus();
  }

  function selectOption(opt: SelectOption) {
    if (opt.disabled) return;
    const prev = value;
    value = opt.value;
    if (prev !== opt.value) onchange?.(opt.value);
    closeMenu();
  }

  function moveHighlight(delta: number) {
    if (options.length === 0) return;
    let i = highlighted;
    for (let n = 0; n < options.length; n++) {
      i = (i + delta + options.length) % options.length;
      if (!options[i]?.disabled) {
        highlighted = i;
        scrollHighlightedIntoView();
        return;
      }
    }
  }

  function scrollHighlightedIntoView() {
    if (!listEl || highlighted < 0) return;
    const el = listEl.querySelector<HTMLElement>(`[data-index="${highlighted}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }

  function onTriggerClick() {
    if (open) closeMenu();
    else openMenu();
  }

  function onTriggerKeydown(e: KeyboardEvent) {
    if (disabled) return;
    if (
      e.key === "ArrowDown" ||
      e.key === "ArrowUp" ||
      e.key === "Enter" ||
      e.key === " "
    ) {
      e.preventDefault();
      if (!open) openMenu();
      else if (e.key === "ArrowDown") moveHighlight(1);
      else if (e.key === "ArrowUp") moveHighlight(-1);
    }
  }

  function onListKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      closeMenu();
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      moveHighlight(1);
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      moveHighlight(-1);
      return;
    }
    if (e.key === "Home") {
      e.preventDefault();
      highlighted = firstEnabledIndex();
      scrollHighlightedIntoView();
      return;
    }
    if (e.key === "End") {
      e.preventDefault();
      highlighted = lastEnabledIndex();
      scrollHighlightedIntoView();
      return;
    }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      const opt = options[highlighted];
      if (opt) selectOption(opt);
      return;
    }
    if (e.key === "Tab") {
      closeMenu(false);
    }
  }

  // Outside click + scroll/resize while open
  $effect(() => {
    if (!open) return;

    const onPointerDown = (e: PointerEvent) => {
      const t = e.target as Node | null;
      if (triggerEl?.contains(t) || listEl?.contains(t)) return;
      closeMenu(false);
    };
    const onReposition = () => updateMenuPosition();
    const onScroll = (e: Event) => {
      // Ignore scrolls inside the menu itself
      if (listEl && e.target instanceof Node && listEl.contains(e.target)) return;
      updateMenuPosition();
    };

    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("resize", onReposition);
    // capture scroll from any ancestor
    window.addEventListener("scroll", onScroll, true);

    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("resize", onReposition);
      window.removeEventListener("scroll", onScroll, true);
    };
  });
</script>

<div class={cn("relative w-full", className)}>
  <button
    bind:this={triggerEl}
    type="button"
    {id}
    class={cn(
      "flex w-full cursor-pointer items-center justify-between gap-2 rounded-md border border-line bg-input px-2.5 text-left text-fg",
      "focus:outline-none focus-visible:border-focus focus-visible:ring-2 focus-visible:ring-accent-muted",
      "disabled:cursor-not-allowed disabled:opacity-55",
      open && "border-focus ring-2 ring-accent-muted",
      inputSize === "sm" ? "h-control-sm text-label" : "h-control text-body",
    )}
    disabled={disabled}
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls={listId}
    onclick={onTriggerClick}
    onkeydown={onTriggerKeydown}
  >
    <span
      class={cn(
        "min-w-0 flex-1 truncate",
        showingPlaceholder && "text-fg-muted",
      )}
    >
      {displayLabel}
    </span>
    <svg
      class={cn(
        "shrink-0 text-fg-muted transition-transform duration-150",
        inputSize === "sm" ? "size-3" : "size-3.5",
        open && "rotate-180",
      )}
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M4 6l4 4 4-4"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  </button>

  {#if open}
    <ul
      bind:this={listEl}
      id={listId}
      class={cn(
        "z-50 m-0 overflow-auto rounded-md border border-line bg-elevated p-1 shadow-md",
        "focus:outline-none",
      )}
      style={menuStyle}
      role="listbox"
      tabindex="-1"
      aria-activedescendant={highlighted >= 0 && id
        ? `${id}-opt-${highlighted}`
        : undefined}
      onkeydown={onListKeydown}
    >
      {#each options as opt, i (opt.value)}
        {@const isSelected = opt.value === value}
        {@const isActive = i === highlighted}
        <li
          id={id ? `${id}-opt-${i}` : undefined}
          class="list-none"
          role="option"
          aria-selected={isSelected}
          aria-disabled={opt.disabled || undefined}
          data-index={i}
        >
          <button
            type="button"
            tabindex="-1"
            disabled={opt.disabled}
            class={cn(
              "flex w-full items-center rounded-sm px-2.5 text-left transition-colors",
              inputSize === "sm" ? "h-control-sm text-label" : "h-control text-body",
              opt.disabled && "cursor-not-allowed opacity-45",
              !opt.disabled && isActive && "bg-hover text-fg",
              !opt.disabled && isSelected && !isActive && "bg-accent-muted text-accent",
              !opt.disabled && isSelected && isActive && "bg-accent-muted text-accent",
              !opt.disabled && !isSelected && !isActive && "text-fg",
            )}
            onpointerenter={() => {
              if (!opt.disabled) highlighted = i;
            }}
            onclick={() => selectOption(opt)}
          >
            <span class="min-w-0 flex-1 truncate">{opt.label}</span>
            {#if isSelected}
              <svg
                class="ml-2 size-3.5 shrink-0 text-accent"
                viewBox="0 0 16 16"
                fill="none"
                aria-hidden="true"
              >
                <path
                  d="M3.5 8.5l3 3 6-6.5"
                  stroke="currentColor"
                  stroke-width="1.5"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
