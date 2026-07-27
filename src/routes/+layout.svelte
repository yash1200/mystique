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
    // Default 2× root scale (100% larger type/chrome); rem ratios stay design-language.
    // Clear stale 1.x prefs from earlier sessions so default 2× actually applies.
    try {
      const raw = localStorage.getItem("mystique-ui-scale");
      if (raw != null) {
        const n = Number(raw);
        if (Number.isFinite(n) && n < 2) {
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
