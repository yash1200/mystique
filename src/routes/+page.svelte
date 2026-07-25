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
  import {
    deleteProfile,
    listProfiles,
    saveProfile,
    touchProfileConnected,
  } from "$lib/api/profiles";
  import { isTauri } from "$lib/api/tauri";
  import type {
    ConnectionFormDraft,
    ConnectionProfile,
  } from "$lib/types/connection";
  import { saveInputFromDraft } from "$lib/types/connection";

  type ThemePref = "light" | "dark" | "system";
  type ToastState = {
    kind: "info" | "success" | "warning" | "danger";
    message: string;
  } | null;

  let connections = $state<ConnectionProfile[]>([]);
  let selectedId = $state<string | null>(null);
  /** Create form (no selection). */
  let creating = $state(true);
  /** Edit form for the currently selected profile. */
  let editing = $state(false);
  let loading = $state(true);
  let saving = $state(false);
  let loadError = $state<string | null>(null);
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
    }, 3600);
  }

  async function refreshList(preferSelectId?: string | null) {
    const rows = await listProfiles();
    connections = rows.map((r) => ({
      ...r,
      status:
        connections.find((c) => c.id === r.id)?.status ?? "disconnected",
    }));
    if (preferSelectId && connections.some((c) => c.id === preferSelectId)) {
      selectedId = preferSelectId;
      creating = false;
      editing = false;
    } else if (selectedId && !connections.some((c) => c.id === selectedId)) {
      selectedId = null;
      creating = connections.length === 0;
      editing = false;
    } else if (connections.length === 0) {
      creating = true;
      editing = false;
      selectedId = null;
    }
  }

  function selectConnection(id: string) {
    creating = false;
    editing = false;
    selectedId = id;
  }

  function startCreate() {
    creating = true;
    editing = false;
    selectedId = null;
  }

  function startEdit() {
    if (!selected) return;
    editing = true;
    creating = false;
  }

  function cancelForm() {
    if (editing && selectedId) {
      editing = false;
      creating = false;
      return;
    }
    creating = false;
    editing = false;
    if (!selectedId && connections.length === 0) {
      creating = true;
    }
  }

  async function saveConnection(draft: ConnectionFormDraft) {
    if (!isTauri()) {
      showToast(
        "warning",
        "Run with `yarn tauri dev` to persist connections.",
      );
      throw new Error("Not running in Tauri.");
    }

    saving = true;
    try {
      const isUpdate = editing && !!selectedId;
      let input = saveInputFromDraft(draft, {
        id: isUpdate ? selectedId! : undefined,
        existingPasswordSet: selected?.passwordSet,
      });

      let saved = await saveProfile(input);

      // After create, rewrite secretRef to use the real profile id when a password was set.
      if (!isUpdate && draft.password.length > 0) {
        saved = await saveProfile({
          ...input,
          id: saved.id,
          secretRef: `mystique-db/connection/${saved.id}`,
        });
      }

      await refreshList(saved.id);
      showToast(
        "success",
        isUpdate
          ? `Updated “${saved.name}”.`
          : `Saved “${saved.name}” to local database.`,
      );
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      showToast("danger", msg);
      throw err;
    } finally {
      saving = false;
    }
  }

  async function deleteSelected() {
    if (!selected) return;
    if (!isTauri()) {
      showToast("warning", "Run with `yarn tauri dev` to delete connections.");
      return;
    }
    const name = selected.name;
    const id = selected.id;
    if (!confirm(`Delete connection “${name}”? This cannot be undone.`)) {
      return;
    }
    try {
      await deleteProfile(id);
      connections = connections.filter((c) => c.id !== id);
      selectedId = null;
      editing = false;
      creating = true;
      showToast("info", `Deleted “${name}”.`);
    } catch (err) {
      showToast(
        "danger",
        err instanceof Error ? err.message : String(err),
      );
    }
  }

  async function duplicateSelected() {
    if (!selected) return;
    if (!isTauri()) {
      showToast("warning", "Run with `yarn tauri dev` to duplicate connections.");
      return;
    }
    try {
      saving = true;
      const cfg = { ...(selected.pluginConfig ?? {}) };
      const saved = await saveProfile({
        id: null,
        name: `${selected.name}-copy`,
        engine: String(selected.engine),
        colorId: selected.colorId,
        environment: selected.environment,
        readOnly: selected.readOnly,
        notes: selected.notes ?? null,
        pluginConfig: cfg,
        // Do not copy secret_ref — user must re-enter password.
        secretRef: null,
      });
      await refreshList(saved.id);
      showToast("success", `Duplicated as “${saved.name}”.`);
    } catch (err) {
      showToast(
        "danger",
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      saving = false;
    }
  }

  async function connectSelected() {
    if (!selected) return;
    if (!isTauri()) {
      showToast("warning", "Run with `yarn tauri dev` for backend actions.");
      return;
    }
    try {
      const updated = await touchProfileConnected(selected.id);
      connections = connections.map((c) =>
        c.id === updated.id
          ? { ...updated, status: "connected" as const }
          : c,
      );
      showToast(
        "info",
        `Marked “${updated.name}” connected (live engine session not wired yet).`,
      );
    } catch (err) {
      showToast(
        "danger",
        err instanceof Error ? err.message : String(err),
      );
    }
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

    (async () => {
      loading = true;
      loadError = null;
      try {
        if (!isTauri()) {
          loadError =
            "Desktop backend not available. Use `yarn tauri dev` to load and save connections.";
          connections = [];
          creating = true;
          return;
        }
        await refreshList();
        if (connections.length === 0) {
          creating = true;
          selectedId = null;
        } else {
          // Default: no selection — create form, per product UX
          creating = true;
          selectedId = null;
        }
      } catch (err) {
        loadError = err instanceof Error ? err.message : String(err);
        showToast("danger", loadError);
      } finally {
        loading = false;
      }
    })();

    return () => mq.removeEventListener("change", onScheme);
  });
</script>

<div class="flex h-screen w-screen overflow-hidden bg-app">
  <ConnectionSidebar
    {connections}
    {selectedId}
    creating={creating || editing}
    onselect={selectConnection}
    onnew={startCreate}
  />

  <main class="flex h-full min-w-0 flex-1 flex-col">
    <div
      class="flex h-topbar shrink-0 items-center justify-between gap-3 border-b border-line bg-panel px-4"
    >
      <div class="flex min-w-0 items-center gap-1.5 text-caption text-fg">
        <span class="text-fg-muted">Home</span>
        <span class="text-fg-muted">/</span>
        {#if creating}
          <span>New connection</span>
        {:else if editing && selected}
          <span>Edit · {selected.name}</span>
        {:else if selected}
          <span>{selected.name}</span>
        {:else}
          <span>Connections</span>
        {/if}
        {#if loading}
          <span class="text-fg-muted">· Loading…</span>
        {/if}
      </div>
      <ThemeToggle preference={themePref} onchange={setThemePref} />
    </div>

    <div class="flex-1 overflow-auto bg-app-wash">
      {#if loadError && connections.length === 0 && !isTauri()}
        <EmptyState
          title="Backend required"
          description={loadError}
        >
          {#snippet actions()}
            <Button variant="primary" onclick={startCreate}>
              Preview form only
            </Button>
          {/snippet}
        </EmptyState>
      {:else if creating}
        <NewConnectionPanel
          initial={null}
          {saving}
          oncancel={connections.length > 0 ? cancelForm : undefined}
          onsave={saveConnection}
          ontest={() =>
            showToast(
              "info",
              "Test connection will run when the engine plugin is wired.",
            )}
        />
      {:else if editing && selected}
        <NewConnectionPanel
          initial={selected}
          {saving}
          oncancel={cancelForm}
          onsave={saveConnection}
          ontest={() =>
            showToast(
              "info",
              "Test connection will run when the engine plugin is wired.",
            )}
        />
      {:else if selected}
        <ConnectionDetail
          connection={selected}
          onconnect={connectSelected}
          onduplicate={duplicateSelected}
          ondelete={deleteSelected}
          onedit={startEdit}
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
