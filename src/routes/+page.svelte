<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import ConnectionSidebar from "$lib/components/home/ConnectionSidebar.svelte";
  import ConnectionDetail from "$lib/components/home/ConnectionDetail.svelte";
  import NewConnectionPanel from "$lib/components/home/NewConnectionPanel.svelte";
  import {
    Button,
    Toast,
  } from "$lib/components/ui";
  import {
    deleteProfile,
    listProfiles,
    saveProfile,
    touchProfileConnected,
  } from "$lib/api/profiles";
  import { openEngineSession, testEngineConnection } from "$lib/api/engines";
  import { isTauri } from "$lib/api/tauri";
  import { workspaceSession } from "$lib/stores/workspace-session.svelte";
  import type {
    ConnectionFormDraft,
    ConnectionProfile,
  } from "$lib/types/connection";
  import {
    draftFromProfile,
    pluginConfigFromDraft,
    pluginConfigForSession,
    saveInputFromDraft,
  } from "$lib/types/connection";

  type ThemePref = "light" | "dark" | "system";
  type ToastState = {
    kind: "info" | "success" | "warning" | "danger";
    message: string;
  } | null;

  let connections = $state<ConnectionProfile[]>([]);
  let selectedId = $state<string | null>(null);
  /** Create form is the home landing (no selection). */
  let creating = $state(true);
  /** Edit form for the currently selected profile. */
  let editing = $state(false);
  /** Remount the create form to reset fields (Home / New / Cancel). */
  let createFormKey = $state(0);
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

  /** Landing: create-connection form. Connections live in the sidebar. */
  function enterCreateMode() {
    selectedId = null;
    creating = true;
    editing = false;
    testResult = null;
    createFormKey += 1;
  }

  async function refreshList(preferSelectId?: string | null) {
    const rows = await listProfiles();
    connections = rows.map((r) => ({
      ...r,
      status:
        connections.find((c) => c.id === r.id)?.status ?? "disconnected",
    }));
    if (preferSelectId && connections.some((c) => c.id === preferSelectId)) {
      // After save/duplicate: show the new/updated profile detail.
      selectedId = preferSelectId;
      creating = false;
      editing = false;
    } else if (!selectedId || !connections.some((c) => c.id === selectedId)) {
      if (!creating && !editing) {
        enterCreateMode();
      }
    }
  }

  function selectConnection(id: string) {
    creating = false;
    editing = false;
    testResult = null;
    selectedId = id;
  }

  function startCreate() {
    enterCreateMode();
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
        "Backend not available. Run with `npm run tauri dev` to test a real database connection.";
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
      // Tauri may throw a string, Error, or { message } object
      let raw: string;
      if (err instanceof Error) {
        raw = err.message;
      } else if (typeof err === "string") {
        raw = err;
      } else if (err && typeof err === "object" && "message" in err) {
        raw = String((err as { message: unknown }).message);
      } else {
        raw = String(err);
      }
      const msg = `Connection failed: ${raw}`;
      testResult = { ok: false, message: msg };
      showToast("danger", msg);
    } finally {
      testing = false;
    }
  }

  function cancelForm() {
    // Leave edit → back to that connection's detail.
    if (editing && selectedId) {
      editing = false;
      creating = false;
      testResult = null;
      return;
    }
    // Cancel create → reset the form.
    enterCreateMode();
  }

  async function saveConnection(draft: ConnectionFormDraft) {
    saving = true;
    try {
      const isUpdate = editing && !!selectedId;
      const input = saveInputFromDraft(draft, {
        id: isUpdate ? selectedId! : undefined,
        existingPasswordSet: selected?.passwordSet,
      });

      // Password is persisted plaintext with the profile (local SQLite).
      const saved = await saveProfile(input);

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

  let deleting = $state(false);
  /** In-app confirm — `window.confirm` often opens *behind* the Tauri window on macOS. */
  let pendingDelete = $state<{ id: string; name: string } | null>(null);

  function requestDeleteSelected() {
    if (!selected || deleting) return;
    pendingDelete = { id: selected.id, name: selected.name };
  }

  function cancelPendingDelete() {
    if (deleting) return;
    pendingDelete = null;
  }

  function formatInvokeError(err: unknown): string {
    if (err instanceof Error) return err.message;
    if (typeof err === "string") return err;
    if (err && typeof err === "object") {
      const o = err as Record<string, unknown>;
      if (typeof o.message === "string") return o.message;
      try {
        return JSON.stringify(err);
      } catch {
        /* fall through */
      }
    }
    return String(err);
  }

  async function confirmDeleteSelected() {
    if (!pendingDelete) return;
    const { id, name } = pendingDelete;
    deleting = true;
    try {
      await deleteProfile(id);
      // Re-fetch from SQLite so UI matches disk (do not only filter local state).
      await refreshList();
      // refreshList clears invalid selection → create mode.
      pendingDelete = null;
      showToast("info", `Deleted “${name}”.`);
    } catch (err) {
      showToast("danger", formatInvokeError(err));
    } finally {
      deleting = false;
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
        password: selected.password ?? null,
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

  async function openWorkspace(
    profile: ConnectionProfile,
    engineSessionId?: string | null,
  ) {
    workspaceSession.connect(
      { ...profile, status: "connected" },
      engineSessionId,
    );
    await goto(`/workspace?c=${encodeURIComponent(profile.id)}`);
  }

  async function connectSelected() {
    if (!selected) return;

    // FE preview: open workspace with dummy tables (no live engine).
    if (!isTauri()) {
      connections = connections.map((c) =>
        c.id === selected.id ? { ...c, status: "connected" as const } : c,
      );
      showToast(
        "info",
        "Opening workspace with sample data (FE preview — run `npm run tauri dev` for live DB).",
      );
      await openWorkspace(selected);
      return;
    }

    testing = true;
    testResult = null;
    try {
      // Live test first — proves the DB is reachable with stored config + password.
      const draft = draftFromProfile(selected);
      const pluginConfig = pluginConfigForSession(selected);
      const password =
        draft.password.trim().length > 0
          ? draft.password
          : selected.password?.trim() || null;

      const result = await testEngineConnection({
        engine: String(selected.engine),
        pluginConfig,
        password,
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

      // Open a pooled engine session for query execute in the workspace.
      const session = await openEngineSession({
        engine: String(selected.engine),
        pluginConfig,
        password,
        pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
        sessionId: selected.id,
      });

      const version = result.serverVersion ? ` · ${result.serverVersion}` : "";
      showToast(
        "success",
        `Connection established to ${result.engine} (${result.latencyMs}ms)${version}`,
      );
      testResult = {
        ok: true,
        message: `Connection established (${result.latencyMs}ms)${version}`,
      };
      await openWorkspace(
        { ...updated, status: "connected" },
        session.sessionId,
      );
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
        enterCreateMode();
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
    themePreference={themePref}
    onthemechange={setThemePref}
  />

  <main class="h-full min-w-0 flex-1 overflow-auto bg-app-wash">
    {#if editing && selected}
      <NewConnectionPanel
        initial={selected}
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
        ondelete={requestDeleteSelected}
        onedit={startEdit}
        onbacktonew={enterCreateMode}
      />
    {:else}
      {#key createFormKey}
        <NewConnectionPanel
          initial={null}
          {saving}
          {testing}
          {testResult}
          onsave={saveConnection}
          ontest={handleTestConnection}
        />
      {/key}
    {/if}
  </main>

  {#if pendingDelete}
    <div class="fixed inset-0 z-50 flex items-center justify-center p-4">
      <button
        type="button"
        class="absolute inset-0 border-none bg-app/70"
        aria-label="Dismiss delete dialog"
        onclick={cancelPendingDelete}
        disabled={deleting}
      ></button>
      <div
        class="relative z-10 w-full max-w-md rounded-xl border border-line bg-elevated p-5 shadow-md"
        role="dialog"
        aria-modal="true"
        aria-labelledby="delete-conn-title"
      >
        <h2 id="delete-conn-title" class="m-0 text-title font-semibold text-fg">
          Delete connection?
        </h2>
        <p class="mt-2 text-body text-fg-secondary">
          Delete “{pendingDelete.name}”? Saved queries for this connection will
          also be removed. This cannot be undone.
        </p>
        <div class="mt-5 flex justify-end gap-2">
          <Button variant="ghost" onclick={cancelPendingDelete} disabled={deleting}>
            Cancel
          </Button>
          <Button
            variant="danger"
            onclick={confirmDeleteSelected}
            disabled={deleting}
          >
            {deleting ? "Deleting…" : "Delete"}
          </Button>
        </div>
      </div>
    </div>
  {/if}

  {#if toast}
    <Toast message={toast.message} tone={toast.kind} />
  {/if}
</div>
