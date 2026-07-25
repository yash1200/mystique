<script lang="ts">
  import type { ConnectionProfile } from "$lib/types/connection";
  import {
    colorHex,
    connectionEndpoint,
    engineLabel,
  } from "$lib/types/connection";
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
  const status = $derived(connection.status ?? "disconnected");
  const endpoint = $derived(connectionEndpoint(connection));
</script>

<button
  type="button"
  class={cn(
    "min-h-row w-full rounded-md border border-transparent py-2 pr-3 pl-0 text-left text-fg transition-colors",
    // Selected keeps connection tint; hover must not wipe it with bg-hover
    selected
      ? "conn-tint hover:conn-tint"
      : "bg-transparent hover:bg-hover",
  )}
  style="--connection-color: {hex}"
  {onclick}
  aria-current={selected ? "true" : undefined}
>
  <ConnectionStripe color={hex} {selected}>
    <div class="flex min-w-0 items-center gap-2">
      <StatusDot {status} />
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
            {endpoint}
          </span>
        </div>
      </div>
      <span class="hidden shrink-0 text-caption text-fg-muted">
        {engineLabel(connection.engine)}
      </span>
    </div>
  </ConnectionStripe>
</button>
