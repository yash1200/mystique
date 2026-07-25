<script lang="ts">
  import type { ConnectionProfile } from "$lib/types/connection";
  import {
    colorHex,
    configBool,
    configNum,
    configStr,
    engineLabel,
  } from "$lib/types/connection";
  import {
    Button,
    Card,
    ContentPanel,
    DefinitionList,
    EngineBadge,
    Inline,
    Pill,
  } from "$lib/components/ui";
  import { cn } from "$lib/utils/cn";

  interface Props {
    connection: ConnectionProfile;
    onedit?: () => void;
    onconnect?: () => void;
    onduplicate?: () => void;
    ondelete?: () => void;
    onbacktonew?: () => void;
  }

  let {
    connection,
    onedit,
    onconnect,
    onduplicate,
    ondelete,
    onbacktonew,
  }: Props = $props();

  const hex = $derived(colorHex(connection.colorId));
  const status = $derived(connection.status ?? "disconnected");
  const cfg = $derived(connection.pluginConfig ?? {});

  const statusText = $derived(
    status === "connected"
      ? "Connected"
      : status === "error"
        ? "Last attempt failed"
        : "Disconnected",
  );

  const envLabel = $derived(
    connection.environment === "production"
      ? "Production"
      : connection.environment === "staging"
        ? "Staging"
        : "Local",
  );

  function formatDate(iso: string | null | undefined): string {
    if (!iso) return "Never";
    try {
      return new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(new Date(iso));
    } catch {
      return iso;
    }
  }

  const connectionFields = $derived([
    { label: "Host", value: configStr(cfg, "host", "—"), mono: true },
    {
      label: "Port",
      value: configNum(cfg, "port") ? String(configNum(cfg, "port")) : "—",
      mono: true,
    },
    { label: "Database", value: configStr(cfg, "database", "—"), mono: true },
    { label: "Username", value: configStr(cfg, "username", "—"), mono: true },
    {
      label: "Password",
      value: connection.passwordSet ? "•••••••• (ref set)" : "Not set",
    },
    {
      label: "SSL",
      value:
        configBool(cfg, "ssl") || configStr(cfg, "sslMode") === "require"
          ? "Enabled"
          : "Disabled",
    },
  ]);

  const workspaceFields = $derived([
    { label: "Environment", value: envLabel },
    {
      label: "Access mode",
      value: connection.readOnly
        ? "Read-only (writes blocked)"
        : "Read / write",
    },
    { label: "Color", value: connection.colorId },
    {
      label: "Last connected",
      value: formatDate(connection.lastConnectedAt),
    },
    {
      label: "Notes",
      value: connection.notes?.trim() ? connection.notes : "—",
      full: true,
    },
  ]);
</script>

<ContentPanel size="lg" ariaLabel="Connection details">
  <div style="--connection-color: {hex}">
    <header
      class="mb-6 rounded-xl border border-line border-l-4 border-l-(--connection-color) bg-elevated p-5 shadow-sm"
    >
      <div class="flex flex-wrap items-start justify-between gap-4">
        <div class="flex min-w-0 items-start gap-3">
          <span
            class="mt-1.5 size-3 shrink-0 rounded-full bg-conn conn-ring"
            aria-hidden="true"
          ></span>
          <div>
            <div class="flex flex-wrap items-center gap-2">
              <h1 class="m-0 text-display font-semibold tracking-tight text-fg">
                {connection.name}
              </h1>
              {#if connection.environment === "production"}
                <Pill tone="warning" uppercase>PROD</Pill>
              {/if}
              {#if connection.readOnly}
                <Pill>Read-only</Pill>
              {/if}
            </div>
            <p
              class="mt-2 flex flex-wrap items-center gap-2 text-body text-fg-secondary"
            >
              <EngineBadge engine={connection.engine} size="md" />
              <span>{engineLabel(connection.engine)}</span>
              <span class="text-fg-muted">·</span>
              <span
                class={cn(
                  status === "connected" && "text-success",
                  status === "error" && "text-danger",
                  status === "disconnected" && "text-fg-muted",
                )}
              >
                {statusText}
              </span>
            </p>
          </div>
        </div>
        <Inline gap="sm" justify="end">
          <Button variant="ghost" onclick={onedit}>Edit</Button>
          <Button variant="ghost" onclick={onduplicate}>Duplicate</Button>
          <Button variant="danger-ghost" onclick={ondelete}>Delete</Button>
          <Button variant="primary" onclick={onconnect}>
            {status === "connected" ? "Open workspace" : "Connect"}
          </Button>
        </Inline>
      </div>
    </header>

    <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
      <Card title="Connection">
        <DefinitionList items={connectionFields} />
      </Card>

      <Card title="Workspace defaults">
        <DefinitionList items={workspaceFields} />
        <div
          class="mt-3 flex items-center gap-2 text-label capitalize text-fg-muted"
        >
          <span
            class="size-3.5 rounded-sm border border-line bg-conn"
            aria-hidden="true"
          ></span>
          Identity color
        </div>
      </Card>

      <Card title="Capabilities (preview)" class="md:col-span-2">
        <p class="mb-3 text-label text-fg-muted">
          Driven by the engine plugin after connect — shown here for UI preview
          only.
        </p>
        <ul class="m-0 flex list-none flex-wrap gap-2 p-0">
          {#each ["Explorer · schemas & tables", "Data · paginated preview", "Query · SQL (read-only policy ready)", "Model · ER from PK / FK", "Export · CSV / TSV"] as cap}
            <li
              class="rounded-full border border-line bg-input px-2.5 py-1.5 text-label text-fg-secondary"
            >
              {cap}
            </li>
          {/each}
        </ul>
      </Card>
    </div>

    <p class="mt-5 text-caption text-fg-muted">
      Profile is stored in local SQLite. Live engine connect ships with the
      Postgres plugin.
      <button
        type="button"
        class="border-none bg-transparent p-0 text-caption text-accent hover:underline"
        onclick={onbacktonew}
      >
        Create another connection
      </button>
    </p>
  </div>
</ContentPanel>
