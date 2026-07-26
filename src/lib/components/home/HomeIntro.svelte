<script lang="ts">
  import type { ConnectionProfile } from "$lib/types/connection";
  import {
    colorHex,
    connectionEndpoint,
    engineLabel,
  } from "$lib/types/connection";
  import {
    Button,
    ContentPanel,
    EmptyState,
    EngineBadge,
    PageHeader,
    Pill,
    StatusDot,
  } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";

  interface Props {
    connections: ConnectionProfile[];
    loading?: boolean;
    connectingId?: string | null;
    oncreateConnection: () => void;
    oncreateEr: () => void;
    onopen: (id: string) => void;
    onconnect: (id: string) => void;
  }

  let {
    connections,
    loading = false,
    connectingId = null,
    oncreateConnection,
    oncreateEr,
    onopen,
    onconnect,
  }: Props = $props();

  /** Newest first when lastConnectedAt is set. */
  const sorted = $derived(
    [...connections].sort((a, b) => {
      const ta = a.lastConnectedAt ? Date.parse(a.lastConnectedAt) : 0;
      const tb = b.lastConnectedAt ? Date.parse(b.lastConnectedAt) : 0;
      if (tb !== ta) return tb - ta;
      return (b.updatedAt || "").localeCompare(a.updatedAt || "");
    }),
  );
</script>

<ContentPanel size="lg" ariaLabel="Home">
  <PageHeader
    title="Welcome to MystiqueDB"
    description="Connect to a database or design a model. Pick up where you left off from your saved connections."
  />

  <!-- Primary actions -->
  <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
    <button
      type="button"
      class={cn(
        "flex min-h-[8.5rem] flex-col rounded-xl border border-line bg-panel p-5 text-left transition-colors duration-150",
        "hover:border-line-strong hover:bg-hover",
        "focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-2",
      )}
      onclick={oncreateConnection}
    >
      <span
        class="mb-3 grid size-9 place-items-center rounded-lg bg-accent-muted text-body font-semibold text-accent"
        aria-hidden="true"
      >
        +
      </span>
      <span class="text-body-lg font-semibold text-fg">Create connection</span>
      <span class="mt-1.5 text-body text-fg-secondary">
        Add PostgreSQL, MongoDB, or SQLite and save a profile for later.
      </span>
    </button>

    <button
      type="button"
      class={cn(
        "flex min-h-[8.5rem] flex-col rounded-xl border border-line bg-panel p-5 text-left transition-colors duration-150",
        "hover:border-line-strong hover:bg-hover",
        "focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-2",
      )}
      onclick={oncreateEr}
    >
      <span
        class="mb-3 grid size-9 place-items-center rounded-lg border border-line bg-input font-mono text-label font-semibold text-fg-secondary"
        aria-hidden="true"
      >
        ER
      </span>
      <span class="text-body-lg font-semibold text-fg">Create ER</span>
      <span class="mt-1.5 text-body text-fg-secondary">
        Open the model designer — choose SQL, NoSQL, or Redis and sketch tables,
        collections, or key patterns (FE canvas).
      </span>
    </button>
  </div>

  <!-- Previous connections -->
  <section class="mt-8" aria-labelledby="prev-conn-heading">
    <div class="mb-3 flex items-end justify-between gap-3">
      <div>
        <h2
          id="prev-conn-heading"
          class="m-0 text-label font-semibold tracking-wider text-fg-muted uppercase"
        >
          Your connections
        </h2>
        <p class="mt-1 text-caption text-fg-muted">
          Open a profile or connect to jump into the workspace.
        </p>
      </div>
      {#if connections.length > 0}
        <Button variant="ghost" size="sm" onclick={oncreateConnection}>
          New
        </Button>
      {/if}
    </div>

    {#if loading}
      <div
        class="rounded-xl border border-line bg-panel px-4 py-8 text-center text-body text-fg-muted"
      >
        Loading connections…
      </div>
    {:else if sorted.length === 0}
      <EmptyState
        title="No saved connections yet"
        description="Create a connection to store host, engine, and credentials on this machine."
      >
        {#snippet actions()}
          <Button variant="primary" onclick={oncreateConnection}>
            Create connection
          </Button>
        {/snippet}
      </EmptyState>
    {:else}
      <ul class="m-0 flex list-none flex-col gap-2 p-0">
        {#each sorted as conn (conn.id)}
          {@const hex = colorHex(conn.colorId)}
          {@const status = conn.status ?? "disconnected"}
          {@const busy = connectingId === conn.id}
          <li
            class="flex flex-wrap items-center gap-3 rounded-xl border border-line bg-panel py-2.5 pr-3 pl-0"
            style="--connection-color: {hex}"
          >
            <button
              type="button"
              class="flex min-w-0 flex-1 items-center gap-0 rounded-l-xl border-none bg-transparent py-1 pr-2 pl-0 text-left text-fg transition-colors hover:bg-hover"
              onclick={() => onopen(conn.id)}
            >
              <span
                class="mr-3 h-10 w-1 shrink-0 rounded-r-sm bg-conn"
                aria-hidden="true"
              ></span>
              <StatusDot {status} />
              <div class="ml-2 min-w-0 flex-1">
                <div class="flex min-w-0 flex-wrap items-center gap-1.5">
                  <span class="truncate text-body font-medium">{conn.name}</span>
                  {#if conn.environment === "production"}
                    <Pill tone="warning" uppercase>PROD</Pill>
                  {/if}
                  {#if conn.readOnly}
                    <Pill uppercase>RO</Pill>
                  {/if}
                </div>
                <div class="mt-0.5 flex min-w-0 items-center gap-1.5">
                  <EngineBadge engine={conn.engine} />
                  <span class="truncate font-mono text-caption text-fg-muted">
                    {connectionEndpoint(conn)}
                  </span>
                  <span class="hidden text-caption text-fg-muted sm:inline">
                    · {engineLabel(conn.engine)}
                  </span>
                </div>
              </div>
            </button>
            <div class="flex shrink-0 items-center gap-2 pl-2">
              <Button
                variant="ghost"
                size="sm"
                onclick={() => onopen(conn.id)}
                disabled={busy}
              >
                Details
              </Button>
              <Button
                variant="primary"
                size="sm"
                onclick={() => onconnect(conn.id)}
                disabled={busy}
              >
                {busy ? "Connecting…" : "Connect"}
              </Button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</ContentPanel>
