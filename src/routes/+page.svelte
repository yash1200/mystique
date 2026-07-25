<script lang="ts">
  import { onMount } from "svelte";
  import ConnectionSidebar from "$lib/components/home/ConnectionSidebar.svelte";
  import ConnectionDetail from "$lib/components/home/ConnectionDetail.svelte";
  import NewConnectionPanel from "$lib/components/home/NewConnectionPanel.svelte";
  import {
    Button,
    EmptyState,
    ThemeToggle,
    Toast,
  } from "$lib/components/ui";
  import { MOCK_CONNECTIONS } from "$lib/data/mock-connections";
  import type {
    ConnectionColorId,
    ConnectionProfile,
    EngineId,
    Environment,
  } from "$lib/types/connection";

  type ThemePref = "light" | "dark" | "system";
  type ToastState = {
    kind: "info" | "success" | "warning";
    message: string;
  } | null;

  let connections = $state<ConnectionProfile[]>(
    structuredClone(MOCK_CONNECTIONS),
  );
  /** Home starts with no selection — show create-connection panel. */
  let selectedId = $state<string | null>(null);
  let creating = $state(true);
  let themePref = $state<ThemePref>("dark");
  let toast = $state<ToastState>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  const selected = $derived(
    connections.find((c) => c.id === selectedId) ?? null,
  );

  function resolveTheme(pref: ThemePref): "light" | "dark" {
    if (pref === "system") {
      return window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";
    }
    return pref;
  }

  function applyTheme(pref: ThemePref) {
    const resolved = resolveTheme(pref);
    document.documentElement.dataset.theme = resolved;
    document.documentElement.dataset.density = "compact";
  }

  function setThemePref(pref: ThemePref) {
    themePref = pref;
    try {
      localStorage.setItem("mystique-theme", pref);
    } catch {
      /* ignore */
    }
    applyTheme(pref);
  }

  function showToast(kind: NonNullable<ToastState>["kind"], message: string) {
    toast = { kind, message };
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast = null;
    }, 3200);
  }

  function selectConnection(id: string) {
    creating = false;
    selectedId = id;
  }

  function startCreate() {
    creating = true;
    selectedId = null;
  }

  function cancelCreate() {
    creating = false;
  }

  function saveConnection(draft: {
    name: string;
    engine: EngineId;
    colorId: ConnectionColorId;
    host: string;
    port: number;
    database: string;
    username: string;
    password: string;
    ssl: boolean;
    readOnly: boolean;
    environment: Environment;
    notes: string;
  }) {
    const profile: ConnectionProfile = {
      id: `conn-${crypto.randomUUID().slice(0, 8)}`,
      name: draft.name,
      engine: draft.engine,
      colorId: draft.colorId,
      host: draft.host,
      port: draft.port,
      database: draft.database,
      username: draft.username || "postgres",
      passwordSet: draft.password.length > 0,
      ssl: draft.ssl,
      readOnly: draft.readOnly,
      environment: draft.environment,
      status: "disconnected",
      lastConnectedAt: null,
      notes: draft.notes || undefined,
    };
    connections = [profile, ...connections];
    creating = false;
    selectedId = profile.id;
    showToast("success", `Saved “${profile.name}” (UI only — not persisted).`);
  }

  function connectSelected() {
    if (!selected) return;
    if (selected.status === "error") {
      showToast(
        "warning",
        `Cannot reach ${selected.name} — backend not connected yet.`,
      );
      return;
    }
    connections = connections.map((c) =>
      c.id === selected.id
        ? {
            ...c,
            status: "connected" as const,
            lastConnectedAt: new Date().toISOString(),
          }
        : c,
    );
    showToast(
      "info",
      `Workspace for “${selected.name}” will open here once the backend ships.`,
    );
  }

  function duplicateSelected() {
    if (!selected) return;
    const copy: ConnectionProfile = {
      ...structuredClone(selected),
      id: `conn-${crypto.randomUUID().slice(0, 8)}`,
      name: `${selected.name}-copy`,
      status: "disconnected",
      lastConnectedAt: null,
    };
    connections = [copy, ...connections];
    selectedId = copy.id;
    showToast("success", `Duplicated as “${copy.name}”.`);
  }

  function deleteSelected() {
    if (!selected) return;
    const name = selected.name;
    connections = connections.filter((c) => c.id !== selected.id);
    selectedId = null;
    creating = true;
    showToast("info", `Removed “${name}” from this session.`);
  }

  onMount(() => {
    let stored: ThemePref = "dark";
    try {
      const raw = localStorage.getItem("mystique-theme");
      if (raw === "light" || raw === "dark" || raw === "system") stored = raw;
    } catch {
      /* ignore */
    }
    themePref = stored;
    applyTheme(stored);

    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onScheme = () => {
      if (themePref === "system") applyTheme("system");
    };
    mq.addEventListener("change", onScheme);
    return () => mq.removeEventListener("change", onScheme);
  });
</script>

<div class="flex h-screen w-screen overflow-hidden bg-app">
  <ConnectionSidebar
    {connections}
    {selectedId}
    {creating}
    onselect={selectConnection}
    onnew={startCreate}
  />

  <main class="flex h-full min-w-0 flex-1 flex-col">
    <div class="flex h-topbar shrink-0 items-center justify-between gap-3 border-b border-line bg-panel px-4">
      <div class="flex min-w-0 items-center gap-1.5 text-caption text-fg">
        <span class="text-fg-muted">Home</span>
        <span class="text-fg-muted">/</span>
        {#if creating}
          <span>New connection</span>
        {:else if selected}
          <span>{selected.name}</span>
        {:else}
          <span>Connections</span>
        {/if}
      </div>
      <ThemeToggle preference={themePref} onchange={setThemePref} />
    </div>

    <div class="flex-1 overflow-auto bg-app-wash">
      {#if creating}
        <NewConnectionPanel
          oncancel={connections.length > 0 ? cancelCreate : undefined}
          onsave={saveConnection}
          ontest={() =>
            showToast(
              "info",
              "Test connection is UI-only until the Postgres plugin is wired.",
            )}
        />
      {:else if selected}
        <ConnectionDetail
          connection={selected}
          onconnect={connectSelected}
          onduplicate={duplicateSelected}
          ondelete={deleteSelected}
          onedit={() =>
            showToast("info", "Edit form will reuse the new-connection UI.")}
          onbacktonew={startCreate}
        />
      {:else}
        <EmptyState
          title="No connection selected"
          description="Choose one from the left rail, or create a new connection."
        >
          {#snippet actions()}
            <Button variant="primary" onclick={startCreate}>New connection</Button>
          {/snippet}
        </EmptyState>
      {/if}
    </div>
  </main>

  {#if toast}
    <Toast message={toast.message} tone={toast.kind} />
  {/if}
</div>

