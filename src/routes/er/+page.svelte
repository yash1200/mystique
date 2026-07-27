<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import type { DataFamily } from "$lib/types/er";
  import FamilyPicker from "$lib/components/er/FamilyPicker.svelte";
  import ErWorkspace from "$lib/components/er/ErWorkspace.svelte";
  import { ThemeToggle } from "$lib/components/ui";

  type ThemePref = "light" | "dark" | "system";

  let family = $state<DataFamily | null>(null);
  let themePref = $state<ThemePref>("dark");

  function resolveTheme(pref: ThemePref): "light" | "dark" {
    if (pref === "system") {
      return window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";
    }
    return pref;
  }

  function applyTheme(pref: ThemePref) {
    document.documentElement.dataset.theme = resolveTheme(pref);
    document.documentElement.dataset.density = "compact";
  }

  function setThemePref(pref: ThemePref) {
    themePref = pref;
    try {
      localStorage.setItem("mystique-theme", pref);
    } catch {
      /* ignore */
    }
    applyTheme(pref);
  }

  function goHome() {
    goto("/");
  }

  onMount(() => {
    let stored: ThemePref = "dark";
    try {
      const raw = localStorage.getItem("mystique-theme");
      if (raw === "light" || raw === "dark" || raw === "system") stored = raw;
    } catch {
      /* ignore */
    }
    themePref = stored;
    applyTheme(stored);
  });
</script>

<div class="flex h-screen w-screen flex-col overflow-hidden bg-app">
  {#if !family}
    <div
      class="flex h-topbar shrink-0 items-center justify-between gap-3 border-b border-line bg-panel px-5"
    >
      <div class="flex min-w-0 items-center gap-2 text-body text-fg">
        <button
          type="button"
          class="border-none bg-transparent p-0 text-body font-medium text-fg-muted hover:text-accent"
          onclick={goHome}
        >
          ← Home
        </button>
        <span class="text-fg-muted">/</span>
        <span class="font-medium">Create ER</span>
        <span class="text-fg-muted">· choose database family</span>
      </div>
      <ThemeToggle preference={themePref} onchange={setThemePref} />
    </div>
    <div class="flex-1 overflow-auto bg-app-wash">
      <FamilyPicker onselect={(f) => (family = f)} onback={goHome} />
    </div>
  {:else}
    <div class="min-h-0 flex-1">
      <ErWorkspace
        {family}
        onback={goHome}
        onchangeFamily={() => (family = null)}
      >
        {#snippet toolbarEnd()}
          <ThemeToggle preference={themePref} onchange={setThemePref} />
        {/snippet}
      </ErWorkspace>
    </div>
  {/if}
</div>
