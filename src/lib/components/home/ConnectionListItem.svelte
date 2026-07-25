<script lang="ts">
  import type { ConnectionProfile } from "$lib/types/connection";
  import { colorHex, engineLabel } from "$lib/types/connection";
  import {
    ConnectionStripe,
    EngineBadge,
    Pill,
    StatusDot,
  } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";

  interface Props {
    connection: ConnectionProfile;
    selected?: boolean;
    onclick?: () => void;
  }

  let { connection, selected = false, onclick }: Props = $props();

  const hex = $derived(colorHex(connection.colorId));
</script>

<button
  type="button"
  class={cn(
    "min-h-row w-full rounded-md border-none bg-transparent py-2 pr-3 pl-0 text-left text-fg transition-colors hover:bg-hover",
    selected && "conn-tint",
  )}
  style="--connection-color: {hex}"
  {onclick}
  aria-current={selected ? "true" : undefined}
>
  <ConnectionStripe color={hex} {selected} tinted={selected}>
    <div class="flex min-w-0 items-center gap-2">
      <StatusDot status={connection.status} />
      <div class="flex min-w-0 flex-1 flex-col gap-0.5">
        <div class="flex min-w-0 items-center gap-1.5">
          <span class="truncate text-body font-medium">{connection.name}</span>
          {#if connection.environment === "production"}
            <Pill tone="warning" uppercase>PROD</Pill>
          {/if}
          {#if connection.readOnly}
            <Pill uppercase>RO</Pill>
          {/if}
        </div>
        <div class="flex min-w-0 items-center gap-1.5">
          <EngineBadge engine={connection.engine} />
          <span class="truncate font-mono text-caption text-fg-muted">
            {connection.host}:{connection.port}
          </span>
        </div>
      </div>
      <span class="hidden shrink-0 text-caption text-fg-muted">
        {engineLabel(connection.engine)}
      </span>
    </div>
  </ConnectionStripe>
</button>
