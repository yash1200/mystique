<script lang="ts">
  import { onMount } from "svelte";
  import "../app.css";
  import {
    applyUiScale,
    writeScalePreference,
    UI_SCALE_STORAGE_KEY,
  } from "$lib/utils/uiScale";

  let { children } = $props();

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

    // Optional zoom still available; persists until next full reload resets to 1.
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      if (e.key === "=" || e.key === "+") {
        e.preventDefault();
        const cur = Number(document.documentElement.dataset.uiScale) || 1;
        const next = Math.min(2.5, Math.round((cur + 0.1) * 100) / 100);
        writeScalePreference(next);
        applyUiScale(next);
      } else if (e.key === "-") {
        e.preventDefault();
        const cur = Number(document.documentElement.dataset.uiScale) || 1;
        const next = Math.max(1, Math.round((cur - 0.1) * 100) / 100);
        writeScalePreference(next);
        applyUiScale(next);
      } else if (e.key === "0") {
        e.preventDefault();
        writeScalePreference(1);
        applyUiScale(1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

{@render children()}
