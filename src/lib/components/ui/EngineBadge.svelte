<script lang="ts">
  import type { EngineId } from "$lib/types/connection";
  import { engineLabel } from "$lib/types/connection";
  import Badge from "./Badge.svelte";

  interface Props {
    engine: EngineId;
    size?: "sm" | "md";
  }

  let { engine, size = "sm" }: Props = $props();

  const mark = $derived(
    (
      {
        postgres: "Pg",
        sqlite: "Lite",
        mysql: "My",
        mongodb: "Mo",
      } as const
    )[engine],
  );
</script>

<span title={engineLabel(engine)} aria-label={engineLabel(engine)}>
  <Badge {size}>{mark}</Badge>
</span>
