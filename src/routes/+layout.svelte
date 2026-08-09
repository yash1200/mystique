<script lang="ts">
  import { onMount } from "svelte";
  import "../app.css";
  import { isTauri } from "$lib/api/tauri";
  import {
    applyUiScale,
    applyPreferredUiScale,
    writeScalePreference,
  } from "$lib/utils/uiScale";
  import {
    applyStoredAppearance,
    readThemePref,
    applyThemePref,
  } from "$lib/utils/appPrefs";

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
    applyStoredAppearance();
    applyPreferredUiScale();

    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onScheme = () => {
      if (readThemePref() === "system") applyThemePref("system");
    };
    mq.addEventListener("change", onScheme);

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
      mq.removeEventListener("change", onScheme);
      window.removeEventListener("keydown", onKey);
      for (const u of unsubs) u();
    };
  });
</script>

{@render children()}
