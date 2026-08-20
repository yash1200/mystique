<script lang="ts">
  import { cn } from "$lib/utils/cn";

  export interface DefinitionItem {
    label: string;
    value: string;
    mono?: boolean;
    full?: boolean;
  }

  interface Props {
    items: DefinitionItem[];
    columns?: 1 | 2;
  }

  let { items, columns = 2 }: Props = $props();
</script>

<dl
  class={cn(
    "m-0 grid gap-x-4 gap-y-3",
    columns === 1 ? "grid-cols-1" : "grid-cols-1 sm:grid-cols-2",
  )}
>
  {#each items as item (item.label)}
    <div class={cn(item.full && "col-span-full")}>
      <dt class="mb-0.5 text-caption text-fg-muted">{item.label}</dt>
      <dd
        class={cn(
          "m-0 break-words text-body text-fg",
          item.mono && "font-mono text-caption",
        )}
      >
        {item.value}
      </dd>
    </div>
  {/each}
</dl>
