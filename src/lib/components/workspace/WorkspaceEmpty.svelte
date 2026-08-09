<script lang="ts">
  import type { ConnectionProfile } from "$lib/types/connection";
  import {
    colorHex,
    configStr,
    connectionEndpoint,
    engineLabel,
  } from "$lib/types/connection";
  import {
    Badge,
    Button,
    Card,
    DefinitionList,
    EmptyState,
    EngineBadge,
    Kbd,
    Pill,
    StatusDot,
  } from "$lib/components/ui";

  interface Props {
    profile: ConnectionProfile;
    onnewquery?: () => void;
  }

  let { profile, onnewquery }: Props = $props();

  const hex = $derived(colorHex(profile.colorId));
  const endpoint = $derived(connectionEndpoint(profile));
  const database = $derived(configStr(profile.pluginConfig ?? {}, "database"));
  const username = $derived(configStr(profile.pluginConfig ?? {}, "username"));

  const envLabel = $derived(
    profile.environment === "production"
      ? "Production"
      : profile.environment === "staging"
        ? "Staging"
        : "Local",
  );

  const infoItems = $derived.by(() => {
    const items: {
      label: string;
      value: string;
      mono?: boolean;
      full?: boolean;
    }[] = [
      { label: "Engine", value: engineLabel(profile.engine) },
      { label: "Endpoint", value: endpoint, mono: true, full: true },
      { label: "Environment", value: envLabel },
      {
        label: "Access",
        value: profile.readOnly ? "Read-only" : "Read / write",
      },
    ];
    if (database) {
      items.push({ label: "Database", value: database, mono: true });
    }
    if (username) {
      items.push({ label: "Username", value: username, mono: true });
    }
    if (profile.notes?.trim()) {
      items.push({
        label: "Notes",
        value: profile.notes.trim(),
        full: true,
      });
    }
    return items;
  });

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
</script>

<div
  class="flex flex-1 items-center justify-center overflow-auto bg-app-wash p-6"
>
  <div class="w-full max-w-panel-md">
    <div
      class="mb-4 rounded-xl border border-line border-l-4 bg-elevated p-5 shadow-sm"
      style="--connection-color: {hex}; border-left-color: var(--connection-color)"
    >
      <div class="flex flex-wrap items-start justify-between gap-4">
        <div class="flex min-w-0 items-start gap-3">
          <span
            class="mt-1.5 size-3 shrink-0 rounded-full bg-conn"
            style="background-color: var(--connection-color)"
            aria-hidden="true"
          ></span>
          <div class="min-w-0">
            <div class="flex flex-wrap items-center gap-2">
              <h1 class="m-0 text-title font-semibold tracking-tight text-fg">
                {profile.name}
              </h1>
              {#if profile.environment === "production"}
                <Pill tone="warning" uppercase>PROD</Pill>
              {/if}
              {#if profile.readOnly}
                <Pill>Read-only</Pill>
              {/if}
            </div>
            <p
              class="mt-1.5 flex flex-wrap items-center gap-2 text-body text-fg-secondary"
            >
              <EngineBadge engine={profile.engine} size="md" />
              <span>{engineLabel(profile.engine)}</span>
              <span class="text-fg-muted">·</span>
              <StatusDot status="connected" />
              <span class="text-success">Connected</span>
            </p>
            <p class="mt-1 font-mono text-caption text-fg-muted">
              {endpoint}
            </p>
          </div>
        </div>
        <Badge tone="success" size="sm">Session live</Badge>
      </div>
    </div>

    <Card title="Connection details">
      <DefinitionList items={infoItems} />
      <p class="mt-3 text-caption text-fg-muted">
        Last connected · {formatDate(profile.lastConnectedAt)}
      </p>
    </Card>

    <div class="mt-4 rounded-xl border border-line bg-panel px-5 py-6">
      <EmptyState
        title="No tab open"
        description="Double-click a table in the explorer, or open a query."
      >
        {#snippet actions()}
          <Button variant="primary" size="sm" onclick={() => onnewquery?.()}>
            New query
          </Button>
          <span class="self-center text-caption text-fg-muted">
            <Kbd>⌘T</Kbd>
          </span>
        {/snippet}
      </EmptyState>
    </div>
  </div>
</div>
