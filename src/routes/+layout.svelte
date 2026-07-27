<script lang="ts">
  import { onMount } from "svelte";
  import "../app.css";
  import {
    applyPreferredUiScale,
    applyUiScale,
    readScalePreference,
    resolveScale,
    writeScalePreference,
  } from "$lib/utils/uiScale";

  let { children } = $props();

  onMount(() => {
    // Reset accidental 2× prefs from the 53bdc11 experiment so default is 1×.
    try {
      const raw = localStorage.getItem("mystique-ui-scale");
      if (raw != null && raw !== "auto") {
        const n = Number(raw);
        if (Number.isFinite(n) && n >= 2) {
          localStorage.setItem("mystique-ui-scale", "auto");
        }
      }
    } catch {
      /* ignore */
    }
    applyPreferredUiScale();

    const onResize = () => {
      if (readScalePreference() === "auto") applyPreferredUiScale();
    };
    window.addEventListener("resize", onResize);

    // Ctrl/Cmd + / - / 0 — temporary zoom (persists as fixed scale; 0 = auto)
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      if (e.key === "=" || e.key === "+") {
        e.preventDefault();
        const next = Math.min(2.5, resolveScale() + 0.1);
        writeScalePreference(next);
        applyUiScale(next);
      } else if (e.key === "-") {
        e.preventDefault();
        const next = Math.max(1, resolveScale() - 0.1);
        writeScalePreference(next);
        applyUiScale(next);
      } else if (e.key === "0") {
        e.preventDefault();
        writeScalePreference("auto");
        applyPreferredUiScale();
      }
    };
    window.addEventListener("keydown", onKey);

    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("keydown", onKey);
    };
  });
</script>

{@render children()}
