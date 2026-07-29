<script lang="ts">
  import { onMount } from "svelte";
  import "../app.css";
  import { isTauri } from "$lib/api/tauri";
  import {
    applyUiScale,
    writeScalePreference,
    UI_SCALE_STORAGE_KEY,
  } from "$lib/utils/uiScale";

  let { children } = $props();

  function zoomBy(delta: number) {
    const cur = Number(document.documentElement.dataset.uiScale) || 1;
    const next = Math.min(2.5, Math.max(1, Math.round((cur + delta) * 100) / 100));
    writeScalePreference(next);
    applyUiScale(next);
  }

  function zoomReset() {
    writeScalePreference(1);
    applyUiScale(1);
  }

  onMount(() => {
    // Reset accidental 2× prefs from the 53bdc11 experiment so default is 1×.
    try {
      const raw = localStorage.getItem(UI_SCALE_STORAGE_KEY);
      if (raw != null && raw !== "auto") {
        const n = Number(raw);
        if (Number.isFinite(n) && n >= 2) {
          localStorage.setItem(UI_SCALE_STORAGE_KEY, "auto");
        }
      }
    } catch {
      /* ignore */
    }
    writeScalePreference(1);
    applyUiScale(1);

    // Keyboard zoom (browser + when webview receives the keys).
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
      if (e.key === "=" || e.key === "+") {
        e.preventDefault();
        zoomBy(0.1);
      } else if (e.key === "-" || e.key === "_") {
        e.preventDefault();
        zoomBy(-0.1);
      } else if (e.key === "0") {
        e.preventDefault();
        zoomReset();
      }
    };
    window.addEventListener("keydown", onKey);

    // Native View menu (macOS steals ⌘=/⌘-/⌘0 from the webview → emit events).
    const unsubs: Array<() => void> = [];
    let cancelled = false;
    if (isTauri()) {
      void import("@tauri-apps/api/event")
        .then(async ({ listen }) => {
          if (cancelled) return;
          unsubs.push(await listen("ui-zoom-in", () => zoomBy(0.1)));
          unsubs.push(await listen("ui-zoom-out", () => zoomBy(-0.1)));
          unsubs.push(await listen("ui-zoom-reset", () => zoomReset()));
        })
        .catch(() => {
          /* optional */
        });
    }

    return () => {
      cancelled = true;
      window.removeEventListener("keydown", onKey);
      for (const u of unsubs) u();
    };
  });
</script>

{@render children()}
