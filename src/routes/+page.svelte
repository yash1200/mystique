<script lang="ts">
  import { onMount } from "svelte";
  import ConnectionSidebar from "$lib/components/home/ConnectionSidebar.svelte";
  import ConnectionDetail from "$lib/components/home/ConnectionDetail.svelte";
  import NewConnectionPanel from "$lib/components/home/NewConnectionPanel.svelte";
  import EnginePicker from "$lib/components/home/EnginePicker.svelte";
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
  import { testEngineConnection } from "$lib/api/engines";
  import { isTauri } from "$lib/api/tauri";
  import type {
    ConnectionFormDraft,
    ConnectionProfile,
    EngineId,
  } from "$lib/types/connection";
  import {
    draftFromProfile,
    pluginConfigFromDraft,
    saveInputFromDraft,
  } from "$lib/types/connection";

  type ThemePref = "light" | "dark" | "system";
  type ToastState = {
    kind: "info" | "success" | "warning" | "danger";
    message: string;
  } | null;

  /** Create flow: pick engine first, then fill details. */
  type CreateStep = "pick-engine" | "form";

  let connections = $state<ConnectionProfile[]>([]);
  let selectedId = $state<string | null>(null);
  /** Create form (no selection). */
  let creating = $state(true);
  let createStep = $state<CreateStep>("pick-engine");
  let draftEngine = $state<EngineId | null>(null);
  /** Edit form for the currently selected profile. */
  let editing = $state(false);
  let loading = $state(true);
  let saving = $state(false);
  let testing = $state(false);
  /** Last live connectivity result from BE (shown on the form). */
  let testResult = $state<{ ok: boolean; message: string } | null>(null);
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
      createStep = "pick-engine";
      draftEngine = null;
    } else if (selectedId && !connections.some((c) => c.id === selectedId)) {
      selectedId = null;
      creating = connections.length === 0;
      createStep = "pick-engine";
      draftEngine = null;
      editing = false;
    } else if (connections.length === 0) {
      creating = true;
      createStep = "pick-engine";
      draftEngine = null;
      editing = false;
      selectedId = null;
    }
  }

  function selectConnection(id: string) {
    creating = false;
    editing = false;
    createStep = "pick-engine";
    draftEngine = null;
    selectedId = id;
  }

  function startCreate() {
    creating = true;
    editing = false;
    selectedId = null;
    createStep = "pick-engine";
    draftEngine = null;
    testResult = null;
  }

  function pickEngine(engine: EngineId) {
    draftEngine = engine;
    createStep = "form";
    testResult = null;
  }

  function backToEnginePicker() {
    createStep = "pick-engine";
    // keep draftEngine for highlight, or clear:
    // draftEngine stays for aria-selected until form re-enter
  }

  function startEdit() {
    if (!selected) return;
    editing = true;
    creating = false;
    testResult = null;
  }

  /**
   * Live connectivity check via backend engine plugins.
   * Reports established / failed clearly (toast + form banner).
   */
  async function handleTestConnection(draft: ConnectionFormDraft) {
    testResult = null;

    if (!isTauri()) {
      const msg =
        "Backend not available. Run with `yarn tauri dev` (or `npm run tauri dev`) to test a real database connection.";
      testResult = { ok: false, message: msg };
      showToast("warning", msg);
      return;
    }

    testing = true;
    try {
      const pluginConfig = pluginConfigFromDraft(draft);
      // Prefer URI password when embedded; otherwise send form password
      const password =
        draft.password.trim().length > 0 ? draft.password : null;

      const result = await testEngineConnection({
        engine: String(draft.engine),
        pluginConfig,
        password,
        pool: {
          maxSize: 2,
          minIdle: 0,
          acquireTimeoutMs: 8000,
          idleTimeoutSecs: 30,
        },
      });

      if (result.ok) {
        const version = result.serverVersion
          ? ` · ${result.serverVersion}`
          : "";
        const msg = `Connection established to ${result.engine} (${result.latencyMs}ms)${version}. ${result.message}`;
        testResult = { ok: true, message: msg };
        showToast("success", `Connected to ${result.engine} (${result.latencyMs}ms)`);
      } else {
        const msg = `Connection failed: ${result.message}`;
        testResult = { ok: false, message: msg };
        showToast("danger", msg);
      }
    } catch (err) {
      const raw = err instanceof Error ? err.message : String(err);
      const msg = `Connection failed: ${raw}`;
      testResult = { ok: false, message: msg };
      showToast("danger", msg);
    } finally {
      testing = false;
    }
  }

  function cancelForm() {
    if (editing && selectedId) {
      editing = false;
      creating = false;
      return;
    }
    if (creating && createStep === "form") {
      createStep = "pick-engine";
      return;
    }
    creating = false;
    editing = false;
    createStep = "pick-engine";
    draftEngine = null;
    if (!selectedId && connections.length === 0) {
      creating = true;
      createStep = "pick-engine";
    }
  }

  async function saveConnection(draft: ConnectionFormDraft) {
    saving = true;
    try {
      const isUpdate = editing && !!selectedId;
      let input = saveInputFromDraft(draft, {
        id: isUpdate ? selectedId! : undefined,
        existingPasswordSet: selected?.passwordSet,
      });

      let saved = await saveProfile(input);

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
          : isTauri()
            ? `Saved “${saved.name}” to local database.`
            : `Saved “${saved.name}” in this browser (FE preview).`,
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
      createStep = "pick-engine";
      draftEngine = null;
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
      showToast(
        "warning",
        "Backend not available. Run with `yarn tauri dev` to open a live session.",
      );
      return;
    }

    testing = true;
    testResult = null;
    try {
      // Live test first — proves the DB is reachable with stored config.
      // Passwords are not yet loaded from keychain; if password is required,
      // edit the profile and re-enter it, or put credentials in the URI.
      const draft = draftFromProfile(selected);
      const pluginConfig = {
        ...pluginConfigFromDraft(draft),
        ...(selected.pluginConfig ?? {}),
      };

      const result = await testEngineConnection({
        engine: String(selected.engine),
        pluginConfig,
        password: draft.password || null,
        pool: { maxSize: 2, minIdle: 0, acquireTimeoutMs: 8000 },
      });

      if (!result.ok) {
        connections = connections.map((c) =>
          c.id === selected.id ? { ...c, status: "error" as const } : c,
        );
        const msg = `Could not establish connection: ${result.message}`;
        testResult = { ok: false, message: msg };
        showToast("danger", msg);
        return;
      }

      const updated = await touchProfileConnected(selected.id);
      connections = connections.map((c) =>
        c.id === updated.id
          ? { ...updated, status: "connected" as const }
          : c,
      );
      const version = result.serverVersion ? ` · ${result.serverVersion}` : "";
      showToast(
        "success",
        `Connection established to ${result.engine} (${result.latencyMs}ms)${version}`,
      );
      testResult = {
        ok: true,
        message: `Connection established (${result.latencyMs}ms)${version}`,
      };
    } catch (err) {
      connections = connections.map((c) =>
        c.id === selected.id ? { ...c, status: "error" as const } : c,
      );
      showToast(
        "danger",
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      testing = false;
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
        await refreshList();
        if (connections.length === 0) {
          creating = true;
          createStep = "pick-engine";
          selectedId = null;
        } else {
          creating = true;
          createStep = "pick-engine";
          selectedId = null;
        }
        if (!isTauri()) {
          // Soft notice only — FE localStorage still works.
          loadError = null;
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
        {#if creating && createStep === "pick-engine"}
          <span>Choose database</span>
        {:else if creating && createStep === "form"}
          <span>New connection</span>
          {#if draftEngine}
            <span class="text-fg-muted">·</span>
            <span class="font-mono text-fg-secondary">{draftEngine}</span>
          {/if}
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
        {#if !isTauri()}
          <span class="text-fg-muted">· FE preview</span>
        {/if}
      </div>
      <ThemeToggle preference={themePref} onchange={setThemePref} />
    </div>

    <div class="flex-1 overflow-auto bg-app-wash">
      {#if creating && createStep === "pick-engine"}
        <EnginePicker
          selectedEngine={draftEngine}
          onselect={pickEngine}
          oncancel={connections.length > 0 ? cancelForm : undefined}
          showCancel={connections.length > 0}
        />
      {:else if creating && createStep === "form" && draftEngine}
        <NewConnectionPanel
          initial={null}
          lockedEngine={draftEngine}
          {saving}
          {testing}
          {testResult}
          oncancel={connections.length > 0 ? cancelForm : undefined}
          onback={backToEnginePicker}
          onsave={saveConnection}
          ontest={handleTestConnection}
        />
      {:else if editing && selected}
        <NewConnectionPanel
          initial={selected}
          lockedEngine={selected.engine}
          {saving}
          {testing}
          {testResult}
          oncancel={cancelForm}
          onsave={saveConnection}
          ontest={handleTestConnection}
        />
      {:else if selected}
        <ConnectionDetail
          connection={selected}
          {testing}
          onconnect={connectSelected}
          ontest={async () => {
            await handleTestConnection(draftFromProfile(selected));
          }}
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
