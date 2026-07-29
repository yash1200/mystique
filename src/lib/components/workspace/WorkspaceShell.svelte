<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { workspaceSession } from "$lib/stores/workspace-session.svelte";
  import { mockCatalogForEngine } from "$lib/data/mock-workspace";
  import {
    historyItemToEntry,
    MOCK_SAVED_QUERIES,
    type QueryHistoryEntry,
  } from "$lib/data/mock-queries";
  import {
    appendQueryHistory,
    listQueryHistory,
    listRecentQueryHistory,
    type QueryHistoryItem,
  } from "$lib/api/history";
  import {
    isMockCatalog,
    loadLiveCatalog,
    loadTablePreview,
    preferDatabase,
  } from "$lib/api/catalog";
  import { openEngineSession } from "$lib/api/engines";
  import { loadLiveErDiagram } from "$lib/api/liveEr";
  import { isTauri } from "$lib/api/tauri";
  import { pluginConfigForSession } from "$lib/types/connection";
  import type { ConnectionProfile } from "$lib/types/connection";
  import {
    preferredDbForProfile,
    isSessionMissingError,
    modelCacheKey,
  } from "./shell/sessionUtils";
  import { previewBodyForTable } from "./shell/tablePreviewSql";
  import { mountWorkspaceShortcuts } from "./shell/workspaceShortcuts";
  import type { ErDiagramState } from "$lib/types/er";
  import type { DatabaseMeta, TableMeta } from "$lib/types/workspace";
  import SessionsRail, { type SideNavMode } from "./SessionsRail.svelte";
  import WorkspaceSidePane from "./shell/WorkspaceSidePane.svelte";
  import WorkspaceMainPane from "./shell/WorkspaceMainPane.svelte";

  const sessions = $derived(workspaceSession.sessions);
  const activeId = $derived(workspaceSession.activeConnectionId);
  const activeSession = $derived(workspaceSession.activeSession);
  const openTabs = $derived(workspaceSession.openTabs);
  const activeTabId = $derived(workspaceSession.activeTabId);
  const selectedRow = $derived(workspaceSession.selectedRow);

  let catalogs = $state<Record<string, DatabaseMeta[]>>({});
  let selectedDb = $state<Record<string, string>>({});
  let refreshing = $state(false);
  let tableLoading = $state(false);
  let catalogError = $state<string | null>(null);
  let rowSyncTick = $state(0);
  let baselineEpoch = $state(0);
  /** connectionId → last engineSessionId we loaded a live catalog for */
  let liveCatalogKey = $state<Record<string, string>>({});
  /** Prevent overlapping catalog loads for the same connection. */
  let catalogLoadInFlight = $state<Record<string, boolean>>({});

  /** Second-pane mode: schema explorer vs saved queries vs history. */
  let sideMode = $state<SideNavMode>("explorer");
  let selectedQueryId = $state<string | null>(null);
  let selectedHistoryId = $state<string | null>(null);
  let queryHistory = $state<QueryHistoryEntry[]>([]);
  let historyLoading = $state(false);
  let historyError = $state<string | null>(null);
  /** Monotonic token so overlapping history fetches don't clobber newer data. */
  let historyFetchGen = 0;

  /** Live ER / model diagrams keyed by connectionId::database */
  let liveModels = $state<
    Record<
      string,
      { diagram: ErDiagramState; title: string; summary: string; rev: number }
    >
  >({});
  let modelLoading = $state(false);
  let modelError = $state<string | null>(null);

  /** Open / re-open a pooled engine session for a profile (fixes stale old connections). */
  async function reopenEngineSession(
    profile: ConnectionProfile,
  ): Promise<string> {
    const pluginConfig = pluginConfigForSession(profile);
    const password = profile.password?.trim() || null;
    const session = await openEngineSession({
      engine: String(profile.engine),
      pluginConfig,
      password,
      pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
      sessionId: profile.id,
    });
    workspaceSession.connect(profile, session.sessionId);
    return session.sessionId;
  }

  async function ensureLiveCatalog(
    connectionId: string,
    engine: string,
    engineSessionId: string,
    preferredDatabase: string | null,
    force = false,
  ) {
    const existing = catalogs[connectionId];
    const mockStuck = isMockCatalog(engine, existing);
    if (
      !force &&
      !mockStuck &&
      liveCatalogKey[connectionId] === engineSessionId &&
      existing &&
      existing.length > 0
    ) {
      return;
    }
    if (catalogLoadInFlight[connectionId]) return;
    catalogLoadInFlight = { ...catalogLoadInFlight, [connectionId]: true };
    refreshing = true;
    catalogError = null;
    let sessionId = engineSessionId;
    try {
      const live = await loadLiveCatalog(sessionId, engine, {
        preferredDatabase,
      });
      catalogs = { ...catalogs, [connectionId]: live };
      liveCatalogKey = { ...liveCatalogKey, [connectionId]: sessionId };
      const names = live.map((d) => d.name);
      const pick = preferDatabase(names, preferredDatabase);
      if (pick) {
        selectedDb = { ...selectedDb, [connectionId]: pick };
      } else if (live[0]) {
        selectedDb = { ...selectedDb, [connectionId]: live[0].name };
      }
    } catch (e) {
      // Stale pool session (common for older workspace tabs): reopen and retry once.
      const session = sessions.find((s) => s.profile.id === connectionId);
      if (session && isSessionMissingError(e)) {
        try {
          sessionId = await reopenEngineSession(session.profile);
          const live = await loadLiveCatalog(sessionId, engine, {
            preferredDatabase,
          });
          catalogs = { ...catalogs, [connectionId]: live };
          liveCatalogKey = { ...liveCatalogKey, [connectionId]: sessionId };
          const names = live.map((d) => d.name);
          const pick = preferDatabase(names, preferredDatabase);
          if (pick) {
            selectedDb = { ...selectedDb, [connectionId]: pick };
          }
          catalogError = null;
          return;
        } catch (retryErr) {
          catalogError =
            retryErr instanceof Error ? retryErr.message : String(retryErr);
        }
      } else {
        catalogError = e instanceof Error ? e.message : String(e);
      }
      // Do not leave mock data sitting as if it were live for Tauri sessions.
      if (
        !catalogs[connectionId] ||
        isMockCatalog(engine, catalogs[connectionId])
      ) {
        catalogs = {
          ...catalogs,
          [connectionId]: [],
        };
      }
    } finally {
      catalogLoadInFlight = { ...catalogLoadInFlight, [connectionId]: false };
      refreshing = false;
    }
  }

  $effect(() => {
    const session = activeSession;
    if (!session) return;
    const id = session.profile.id;
    const engine = String(session.profile.engine);
    const engineSessionId = session.engineSessionId ?? null;
    const preferred = preferredDbForProfile(session.profile);

    // Catalog only — do not auto-open a Query tab (empty workspace is intentional).
    if (!isTauri()) {
      // Browser preview only.
      if (!catalogs[id]) {
        catalogs = {
          ...catalogs,
          [id]: structuredClone(mockCatalogForEngine(engine)),
        };
      }
      const list = catalogs[id] ?? [];
      if (list.length > 0 && !selectedDb[id]) {
        selectedDb = {
          ...selectedDb,
          [id]:
            preferDatabase(
              list.map((d) => d.name),
              preferred,
            ) || list[0].name,
        };
      }
      return;
    }

    // Live Tauri: ensure engine session + live catalog (including old connections).
    void (async () => {
      let sid = engineSessionId;
      if (!sid) {
        try {
          sid = await reopenEngineSession(session.profile);
        } catch (e) {
          catalogError = e instanceof Error ? e.message : String(e);
          return;
        }
      }
      await ensureLiveCatalog(id, engine, sid, preferred);
    })();
  });

  const databases = $derived(activeId ? (catalogs[activeId] ?? []) : []);

  const currentDatabaseName = $derived(
    activeId ? (selectedDb[activeId] ?? databases[0]?.name ?? "") : "",
  );

  /** Tabs belonging to the active connection (query + tables). */
  const connectionTabs = $derived(
    activeId ? openTabs.filter((t) => t.connectionId === activeId) : [],
  );

  const activeTab = $derived(
    openTabs.find((t) => t.id === activeTabId) ?? null,
  );

  const gridTable = $derived.by(() => {
    if (!activeTab || activeTab.kind !== "table" || !activeTab.tableId) {
      return null;
    }
    const cat = catalogs[activeTab.connectionId];
    if (!cat) {
      const session = sessions.find(
        (s) => s.profile.id === activeTab.connectionId,
      );
      if (!session) return null;
      const list = mockCatalogForEngine(String(session.profile.engine));
      for (const d of list) {
        const t = d.tables.find((x) => x.id === activeTab.tableId);
        if (t) return t;
      }
      return null;
    }
    for (const d of cat) {
      const t = d.tables.find((x) => x.id === activeTab.tableId);
      if (t) return t;
    }
    return null;
  });

  const activeTableIdForExplorer = $derived(
    activeTab &&
      activeTab.kind === "table" &&
      activeTab.connectionId === activeId
      ? (activeTab.tableId ?? null)
      : null,
  );

  const rowSelectionIndex = $derived.by(() => {
    if (!selectedRow || !activeTab || activeTab.kind !== "table") return null;
    if (
      selectedRow.connectionId === activeTab.connectionId &&
      selectedRow.tableId === activeTab.tableId
    ) {
      return selectedRow.rowIndex;
    }
    return null;
  });

  const inspectorData = $derived.by(() => {
    if (sideMode !== "explorer") return null;
    if (rowSelectionIndex === null || !gridTable) return null;
    const row = gridTable.rows[rowSelectionIndex];
    if (!row) return null;
    const title = gridTable.schema
      ? `${gridTable.database}.${gridTable.schema}.${gridTable.name}`
      : `${gridTable.database}.${gridTable.name}`;
    return {
      tableTitle: title,
      columns: gridTable.columns,
      row,
      rowIndex: rowSelectionIndex,
    };
  });

  const selectedSaved = $derived(
    MOCK_SAVED_QUERIES.find((q) => q.id === selectedQueryId) ?? null,
  );
  const selectedHistory = $derived(
    queryHistory.find((h) => h.id === selectedHistoryId) ?? null,
  );

  function historyNameMap(): Map<string, string> {
    return new Map(
      sessions.map((s) => [s.profile.id, s.profile.name] as const),
    );
  }

  function entryFromItem(it: QueryHistoryItem): QueryHistoryEntry {
    const nameById = historyNameMap();
    return historyItemToEntry(
      it,
      (it.connectionId && nameById.get(it.connectionId)) || undefined,
    );
  }

  /** Immediate UI update after a successful append (all engines). */
  function prependHistoryItem(item: QueryHistoryItem | null | undefined) {
    if (!item?.id) {
      void refreshHistory({ preferSelectLatest: true });
      return;
    }
    const entry = entryFromItem(item);
    // Only show for active connection (or global if none).
    if (activeId && entry.connectionId && entry.connectionId !== activeId) {
      return;
    }
    queryHistory = [
      entry,
      ...queryHistory.filter((h) => h.id !== entry.id),
    ].slice(0, 150);
    historyError = null;
    if (sideMode === "history") {
      selectedHistoryId = entry.id;
    }
  }

  async function refreshHistory(opts?: { preferSelectLatest?: boolean }) {
    if (!isTauri()) {
      queryHistory = [];
      return;
    }
    const gen = ++historyFetchGen;
    historyLoading = true;
    try {
      let items: QueryHistoryItem[] = [];
      if (activeId) {
        items = await listQueryHistory(activeId, 150);
        // If connection-scoped list is empty, still try recent and filter —
        // covers older rows / edge cases after engine switches.
        if (items.length === 0) {
          const recent = await listRecentQueryHistory(150);
          items = recent.filter((it) => it.connectionId === activeId);
        }
      } else {
        items = await listRecentQueryHistory(150);
      }
      if (gen !== historyFetchGen) return;
      queryHistory = items.map((it) => entryFromItem(it));
      historyError = null;
      if (opts?.preferSelectLatest && queryHistory[0]) {
        selectedHistoryId = queryHistory[0].id;
      } else if (
        selectedHistoryId &&
        !queryHistory.some((h) => h.id === selectedHistoryId)
      ) {
        selectedHistoryId = queryHistory[0]?.id ?? null;
      } else if (
        !selectedHistoryId &&
        sideMode === "history" &&
        queryHistory[0]
      ) {
        selectedHistoryId = queryHistory[0].id;
      }
    } catch (e) {
      if (gen !== historyFetchGen) return;
      // Keep existing rows; surface the error instead of wiping the list.
      historyError = e instanceof Error ? e.message : String(e);
      console.warn("[mystique] refreshHistory failed:", historyError);
    } finally {
      if (gen === historyFetchGen) historyLoading = false;
    }
  }

  $effect(() => {
    // Reload history when connection changes or history pane is focused.
    const _id = activeId;
    const mode = sideMode;
    if (mode === "history" || _id) {
      void refreshHistory({ preferSelectLatest: mode === "history" });
    }
  });

  function openSqlInEditor(sql: string) {
    if (!activeId) return;
    sideMode = "explorer";
    workspaceSession.openQuery(activeId);
    const tabId = workspaceSession.queryTabId(activeId);
    workspaceSession.setQueryDraft(tabId, sql);
  }

  async function openTable(table: TableMeta) {
    if (!activeId || !activeSession) return;
    sideMode = "explorer";
    workspaceSession.openTable(activeId, table.id, table.name);

    if (!isTauri()) return;

    // Always fetch live preview so reseed / external writes show up.
    tableLoading = true;
    catalogError = null;
    try {
      let engineSessionId = activeSession.engineSessionId;
      if (!engineSessionId) {
        engineSessionId = await reopenEngineSession(activeSession.profile);
      }
      const engine = String(activeSession.profile.engine);
      let preview;
      try {
        preview = await loadTablePreview(engineSessionId, engine, table, 100);
      } catch (e) {
        if (!isSessionMissingError(e)) throw e;
        engineSessionId = await reopenEngineSession(activeSession.profile);
        preview = await loadTablePreview(engineSessionId, engine, table, 100);
      }
      const list = catalogs[activeId] ?? [];
      const next = list.map((db) => ({
        ...db,
        tables: db.tables.map((t) => (t.id === table.id ? preview : t)),
      }));
      catalogs = { ...catalogs, [activeId]: next };
      rowSyncTick += 1;

      // Log table preview as history so Explore clicks show up next to Query runs.
      if (isTauri()) {
        const e = engine.toLowerCase();
        const body = previewBodyForTable(engine, table);
        try {
          const item = await appendQueryHistory({
            connectionId: activeId,
            engine,
            body,
            language: e === "mongodb" || e === "mongo" ? "mql" : "sql",
            success: true,
            rowCount: preview.rows?.length ?? preview.rowCount ?? null,
            durationMs: null,
            errorMessage: null,
          });
          prependHistoryItem(item);
        } catch (histErr) {
          console.warn("[mystique] table preview history:", histErr);
        }
      }
    } catch (e) {
      catalogError = e instanceof Error ? e.message : String(e);
    } finally {
      tableLoading = false;
    }
  }

  function selectDatabase(name: string) {
    if (!activeId) return;
    selectedDb = { ...selectedDb, [activeId]: name };
  }

  /** Open live ER / model diagram for the currently selected database. */
  async function openModelDiagram(force = false) {
    if (!activeSession || !activeId) return;
    sideMode = "explorer";
    const engine = String(activeSession.profile.engine);
    const database = currentDatabaseName || "";
    const title =
      engine.toLowerCase() === "mongodb" || engine.toLowerCase() === "mongo"
        ? `Model · ${database || "db"}`
        : `ER · ${database || "schema"}`;

    const key = modelCacheKey(activeId, database);
    const alreadyOpen =
      activeTab?.kind === "model" &&
      (activeTab.modelDatabase ?? "") === (database.trim() || "");
    workspaceSession.openModel(activeId, database, title);

    // Reuse cache when switching back to the tab; force rebuild when user
    // clicks “ER diagram” while already viewing this model.
    if (!force && !alreadyOpen && liveModels[key]?.diagram?.nodes?.length) {
      modelError = null;
      return;
    }

    if (!isTauri()) {
      modelError =
        "Live ER diagram requires Tauri. Run `npm run tauri dev` and Connect.";
      return;
    }

    modelLoading = true;
    modelError = null;
    try {
      let sid = activeSession.engineSessionId;
      if (!sid) {
        sid = await reopenEngineSession(activeSession.profile);
      }
      const loaded = await loadLiveErDiagram(sid, engine, database || null);
      liveModels = {
        ...liveModels,
        [key]: {
          diagram: loaded.diagram,
          title: loaded.title,
          summary: loaded.summary,
          rev: Date.now(),
        },
      };
      workspaceSession.openModel(activeId, database, loaded.title);
    } catch (e) {
      // Session missing — reopen once
      if (isSessionMissingError(e)) {
        try {
          const sid = await reopenEngineSession(activeSession.profile);
          const loaded = await loadLiveErDiagram(sid, engine, database || null);
          liveModels = {
            ...liveModels,
            [key]: {
              diagram: loaded.diagram,
              title: loaded.title,
              summary: loaded.summary,
              rev: Date.now(),
            },
          };
          workspaceSession.openModel(activeId, database, loaded.title);
          modelError = null;
          return;
        } catch (retryErr) {
          modelError =
            retryErr instanceof Error ? retryErr.message : String(retryErr);
          return;
        }
      }
      modelError = e instanceof Error ? e.message : String(e);
    } finally {
      modelLoading = false;
    }
  }

  const activeModel = $derived.by(() => {
    if (!activeTab || activeTab.kind !== "model" || !activeId) return null;
    const db = activeTab.modelDatabase ?? currentDatabaseName ?? "";
    return liveModels[modelCacheKey(activeId, db)] ?? null;
  });

  async function refreshDatabases() {
    if (!activeSession || !activeId) return;
    const engine = String(activeSession.profile.engine);
    const preferred = preferredDbForProfile(activeSession.profile);

    if (!isTauri()) {
      refreshing = true;
      await new Promise((r) => setTimeout(r, 200));
      const base = structuredClone(mockCatalogForEngine(engine));
      catalogs = { ...catalogs, [activeId]: base };
      if (!base.some((d) => d.name === selectedDb[activeId])) {
        selectedDb = { ...selectedDb, [activeId]: base[0]?.name ?? "" };
      }
      refreshing = false;
      return;
    }

    // Always re-open the pool session so *old* connections pick up URI/DB fixes.
    liveCatalogKey = { ...liveCatalogKey, [activeId]: "" };
    refreshing = true;
    catalogError = null;
    try {
      const sid = await reopenEngineSession(activeSession.profile);
      await ensureLiveCatalog(activeId, engine, sid, preferred, true);
    } catch (e) {
      catalogError = e instanceof Error ? e.message : String(e);
      refreshing = false;
    }
  }

  function createDatabase(name: string) {
    if (!activeId) return;
    const entry: DatabaseMeta = {
      name,
      meta: "created",
      tables: [],
    };
    const list = [...(catalogs[activeId] ?? []), entry];
    catalogs = { ...catalogs, [activeId]: list };
    selectedDb = { ...selectedDb, [activeId]: name };
  }

  function goHome() {
    goto("/");
  }

  function setSideMode(mode: SideNavMode) {
    sideMode = mode;
    if (mode === "saved-queries") {
      selectedHistoryId = null;
      if (!selectedQueryId && MOCK_SAVED_QUERIES[0]) {
        selectedQueryId = MOCK_SAVED_QUERIES[0].id;
      }
    } else if (mode === "history") {
      selectedQueryId = null;
      void refreshHistory({ preferSelectLatest: true });
    }
  }

  function onSelectConnection(id: string) {
    sideMode = "explorer";
    workspaceSession.setActiveConnection(id);
  }

  function onInspectorFieldChange(column: string, value: unknown) {
    if (rowSelectionIndex === null || !gridTable) return;
    const i = rowSelectionIndex;
    const current = gridTable.rows[i] ?? {};
    gridTable.rows[i] = { ...current, [column]: value };
    rowSyncTick += 1;
  }

  function onGridBaselineChange() {
    baselineEpoch += 1;
  }

  function onSelectGridRow(index: number) {
    const tab = workspaceSession.openTabs.find(
      (t) => t.id === workspaceSession.activeTabId,
    );
    if (!tab || tab.kind !== "table" || !tab.tableId) return;
    workspaceSession.selectRow(tab.connectionId, tab.tableId, index);
  }

  function shortcutNewQuery() {
    const id = workspaceSession.activeConnectionId;
    if (!id) return;
    sideMode = "explorer";
    workspaceSession.openNewQuery(id);
  }

  function shortcutCloseTab() {
    workspaceSession.closeActiveTab();
  }

  onMount(() =>
    mountWorkspaceShortcuts({
      onNewQuery: shortcutNewQuery,
      onCloseTab: shortcutCloseTab,
      isTauri: isTauri(),
    }),
  );
</script>

<div class="flex h-screen w-screen overflow-hidden bg-app">
  <SessionsRail
    {sessions}
    {activeId}
    mode={sideMode}
    historyCount={queryHistory.length}
    onselect={onSelectConnection}
    ondisconnect={(id) => {
      workspaceSession.disconnect(id);
      const { [id]: _c, ...restCat } = catalogs;
      const { [id]: _d, ...restDb } = selectedDb;
      const { [id]: _k, ...restKey } = liveCatalogKey;
      const { [id]: _f, ...restFlight } = catalogLoadInFlight;
      catalogs = restCat;
      selectedDb = restDb;
      liveCatalogKey = restKey;
      catalogLoadInFlight = restFlight;
      const nextModels: typeof liveModels = {};
      for (const [k, v] of Object.entries(liveModels)) {
        if (!k.startsWith(`${id}::`)) nextModels[k] = v;
      }
      liveModels = nextModels;
      if (workspaceSession.sessions.length === 0) goto("/");
    }}
    onhome={goHome}
    onmode={setSideMode}
  />

  <WorkspaceSidePane
    {sideMode}
    {activeSession}
    {databases}
    {currentDatabaseName}
    activeTableId={activeTableIdForExplorer}
    {refreshing}
    {modelLoading}
    {queryHistory}
    {selectedQueryId}
    {selectedHistoryId}
    onselectquery={(id) => (selectedQueryId = id)}
    onselecthistory={(id) => (selectedHistoryId = id)}
    onselectdatabase={selectDatabase}
    onrefresh={refreshDatabases}
    oncreatedatabase={createDatabase}
    onopen={openTable}
    onopenmodel={() => void openModelDiagram(false)}
  />

  <WorkspaceMainPane
    {sideMode}
    {activeId}
    {activeSession}
    {connectionTabs}
    {activeTabId}
    {activeTab}
    {currentDatabaseName}
    {selectedSaved}
    {selectedHistory}
    {queryHistory}
    {historyLoading}
    {historyError}
    {catalogError}
    {modelError}
    {modelLoading}
    {activeModel}
    {gridTable}
    {tableLoading}
    {rowSelectionIndex}
    {rowSyncTick}
    {baselineEpoch}
    {inspectorData}
    onopeneditor={openSqlInEditor}
    onhistory={(item) => prependHistoryItem(item)}
    onbaselinechange={onGridBaselineChange}
    onselectrow={onSelectGridRow}
    onopenmodelretry={() => void openModelDiagram(true)}
    oninspectorchange={onInspectorFieldChange}
  />
</div>
