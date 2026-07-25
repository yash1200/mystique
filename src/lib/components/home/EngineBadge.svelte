<!-- Re-export path for existing home imports; prefer `$lib/components/ui/EngineBadge`. -->
<script lang="ts">
  import type { EngineId } from "$lib/types/connection";
  import UiEngineBadge from "$lib/components/ui/EngineBadge.svelte";

  interface Props {
    engine: EngineId;
    size?: "sm" | "md";
  }

  let { engine, size = "sm" }: Props = $props();
</script>

<UiEngineBadge {engine} {size} />
