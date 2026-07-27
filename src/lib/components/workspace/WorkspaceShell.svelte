<script lang="ts">
  import { goto } from "$app/navigation";
  import { workspaceSession } from "$lib/stores/workspace-session.svelte";
  import { mockCatalogForEngine } from "$lib/data/mock-workspace";
  import {
    MOCK_QUERY_HISTORY,
    MOCK_SAVED_QUERIES,
  } from "$lib/data/mock-queries";
  import type { DatabaseMeta, TableMeta } from "$lib/types/workspace";
  import { EmptyState } from "$lib/components/ui";
  import SessionsRail, {
    type SideNavMode,
  } from "./SessionsRail.svelte";
  import ObjectExplorer from "./ObjectExplorer.svelte";
  import QueryListPanel from "./QueryListPanel.svelte";
  import QueryPreview from "./QueryPreview.svelte";
  import QueryCanvas from "./QueryCanvas.svelte";
  import TableTabs from "./TableTabs.svelte";
  import DataGrid from "./DataGrid.svelte";
  import RowInspector from "./RowInspector.svelte";

  const sessions = $derived(workspaceSession.sessions);
  const activeId = $derived(workspaceSession.activeConnectionId);
  const activeSession = $derived(workspaceSession.activeSession);
  const openTabs = $derived(workspaceSession.openTabs);
  const activeTabId = $derived(workspaceSession.activeTabId);
  const selectedRow = $derived(workspaceSession.selectedRow);

  let catalogs = $state<Record<string, DatabaseMeta[]>>({});
  let selectedDb = $state<Record<string, string>>({});
  let refreshing = $state(false);
  let rowSyncTick = $state(0);
  let baselineEpoch = $state(0);

  /** Second-pane mode: schema explorer vs saved queries vs history. */
  let sideMode = $state<SideNavMode>("explorer");
  let selectedQueryId = $state<string | null>(null);
  let selectedHistoryId = $state<string | null>(null);

  $effect(() => {
    const session = activeSession;
    if (!session) return;
    const id = session.profile.id;
    const engine = String(session.profile.engine);

    if (!catalogs[id]) {
      catalogs = {
        ...catalogs,
        [id]: structuredClone(mockCatalogForEngine(engine)),
      };
    }
    const list = catalogs[id] ?? [];
    if (list.length > 0 && !selectedDb[id]) {
      selectedDb = { ...selectedDb, [id]: list[0].name };
    }

    // Always have a Query tab for the active connection.
    const hasQuery = workspaceSession.openTabs.some(
      (t) => t.connectionId === id && t.kind === "query",
    );
    if (!hasQuery) {
      workspaceSession.openQuery(id);
    }
  });

  const databases = $derived(
    activeId ? (catalogs[activeId] ?? []) : [],
  );

  const currentDatabaseName = $derived(
    activeId ? (selectedDb[activeId] ?? databases[0]?.name ?? "") : "",
  );

  /** Tabs belonging to the active connection (query + tables). */
  const connectionTabs = $derived(
    activeId
      ? openTabs.filter((t) => t.connectionId === activeId)
      : [],
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
      const session = sessions.find((s) => s.profile.id === activeTab.connectionId);
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
    MOCK_QUERY_HISTORY.find((h) => h.id === selectedHistoryId) ?? null,
  );

  function openTable(table: TableMeta) {
    if (!activeId) return;
    sideMode = "explorer";
    workspaceSession.openTable(activeId, table.id, table.name);
  }

  function selectDatabase(name: string) {
    if (!activeId) return;
    selectedDb = { ...selectedDb, [activeId]: name };
  }

  async function refreshDatabases() {
    if (!activeSession || !activeId) return;
    refreshing = true;
    await new Promise((r) => setTimeout(r, 450));
    const engine = String(activeSession.profile.engine);
    const base = structuredClone(mockCatalogForEngine(engine));
    const existing = catalogs[activeId] ?? [];
    const seedNames = new Set(base.map((d) => d.name));
    const extras = existing.filter((d) => !seedNames.has(d.name));
    const next = [...base, ...extras];
    catalogs = { ...catalogs, [activeId]: next };
    if (!next.some((d) => d.name === selectedDb[activeId])) {
      selectedDb = { ...selectedDb, [activeId]: next[0]?.name ?? "" };
    }
    refreshing = false;
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
      if (!selectedHistoryId && MOCK_QUERY_HISTORY[0]) {
        selectedHistoryId = MOCK_QUERY_HISTORY[0].id;
      }
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
</script>

<div class="flex h-screen w-screen overflow-hidden bg-app">
  <SessionsRail
    {sessions}
    {activeId}
    mode={sideMode}
    onselect={onSelectConnection}
    ondisconnect={(id) => {
      workspaceSession.disconnect(id);
      const { [id]: _c, ...restCat } = catalogs;
      const { [id]: _d, ...restDb } = selectedDb;
      catalogs = restCat;
      selectedDb = restDb;
      if (workspaceSession.sessions.length === 0) goto("/");
    }}
    onhome={goHome}
    onmode={setSideMode}
  />

  {#if sideMode === "saved-queries"}
    <QueryListPanel
      mode="saved-queries"
      savedQueries={MOCK_SAVED_QUERIES}
      history={MOCK_QUERY_HISTORY}
      selectedId={selectedQueryId}
      onselect={(id) => {
        selectedQueryId = id;
      }}
    />
  {:else if sideMode === "history"}
    <QueryListPanel
      mode="history"
      savedQueries={MOCK_SAVED_QUERIES}
      history={MOCK_QUERY_HISTORY}
      selectedId={selectedHistoryId}
      onselect={(id) => {
        selectedHistoryId = id;
      }}
    />
  {:else if activeSession}
    <ObjectExplorer
      engine={String(activeSession.profile.engine)}
      {databases}
      selectedDatabase={currentDatabaseName}
      activeTableId={activeTableIdForExplorer}
      {refreshing}
      onselectdatabase={selectDatabase}
      onrefresh={refreshDatabases}
      oncreatedatabase={createDatabase}
      onopen={openTable}
    />
  {:else}
    <aside
      class="flex w-60 shrink-0 flex-col items-center justify-center border-r border-line bg-panel px-4 text-center"
    >
      <p class="text-caption text-fg-muted">Select a session</p>
    </aside>
  {/if}

  <main class="flex min-w-0 flex-1 flex-col">
    {#if sideMode === "explorer" && activeId}
      <TableTabs
        tabs={connectionTabs}
        {activeTabId}
        onactivate={(id) => workspaceSession.setActiveTab(id)}
        onclose={(id) => workspaceSession.closeTab(id)}
        onopenquery={() => workspaceSession.openQuery(activeId)}
      />
    {/if}

    <div class="flex min-h-0 flex-1">
      {#if sideMode === "saved-queries"}
        {#if selectedSaved}
          <QueryPreview kind="saved" saved={selectedSaved} />
        {:else}
          <div class="flex flex-1 items-center justify-center p-8">
            <EmptyState
              title="Saved queries"
              description="Pick a saved query from the list to preview its SQL."
            />
          </div>
        {/if}
      {:else if sideMode === "history"}
        {#if selectedHistory}
          <QueryPreview kind="history" history={selectedHistory} />
        {:else}
          <div class="flex flex-1 items-center justify-center p-8">
            <EmptyState
              title="Query history"
              description="Pick a past run from the list to inspect the statement."
            />
          </div>
        {/if}
      {:else if activeSession && activeTab?.kind === "query"}
        {#key activeTab.id}
          <QueryCanvas
            connectionId={activeSession.profile.id}
            connectionName={activeSession.profile.name}
            engine={String(activeSession.profile.engine)}
            profileReadOnly={!!activeSession.profile.readOnly}
            text={workspaceSession.queryDrafts[activeTab.id] ?? ""}
            writeEnabled={workspaceSession.queryWriteMode[activeTab.id] ?? false}
            ontextchange={(v) => workspaceSession.setQueryDraft(activeTab.id, v)}
            onwritechange={(v) =>
              workspaceSession.setQueryWriteMode(activeTab.id, v)}
          />
        {/key}
      {:else}
        <div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
          {#if gridTable && activeTab?.kind === "table"}
            {#key gridTable.id}
              <DataGrid
                table={gridTable}
                selectedRowIndex={rowSelectionIndex}
                {rowSyncTick}
                onbaselinechange={onGridBaselineChange}
                onselectrow={onSelectGridRow}
              />
            {/key}
          {:else}
            <div class="flex flex-1 items-center justify-center p-8">
              <EmptyState
                title="Query or open a table"
                description="Use the Query tab to run statements, or pick a table from the explorer."
              />
            </div>
          {/if}
        </div>

        {#if inspectorData}
          <RowInspector
            tableTitle={inspectorData.tableTitle}
            columns={inspectorData.columns}
            row={inspectorData.row}
            rowIndex={inspectorData.rowIndex}
            {baselineEpoch}
            onclose={() => workspaceSession.clearRowSelection()}
            onchange={onInspectorFieldChange}
          />
        {/if}
      {/if}
    </div>
  </main>
</div>
