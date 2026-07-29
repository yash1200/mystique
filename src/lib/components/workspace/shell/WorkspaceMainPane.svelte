<script lang="ts">
  import { workspaceSession } from "$lib/stores/workspace-session.svelte";
  import type { ConnectedSession } from "$lib/stores/workspace-session.svelte";
  import type { WorkspaceTab } from "$lib/types/workspace";
  import type { TableMeta } from "$lib/types/workspace";
  import type { ErDiagramState } from "$lib/types/er";
  import {
    MOCK_SAVED_QUERIES,
    type QueryHistoryEntry,
    type SavedQuery,
  } from "$lib/data/mock-queries";
  import type { QueryHistoryItem } from "$lib/api/history";
  import { familyForEngine } from "$lib/api/liveEr";
  import type { SideNavMode } from "../SessionsRail.svelte";
  import { EmptyState } from "$lib/components/ui";
  import ErWorkspace from "$lib/components/er/ErWorkspace.svelte";
  import TableTabs from "../TableTabs.svelte";
  import QueryCanvas from "../QueryCanvas.svelte";
  import QueryPreview from "../QueryPreview.svelte";
  import DataGrid from "../DataGrid.svelte";
  import RowInspector from "../RowInspector.svelte";
  import WorkspaceEmpty from "../WorkspaceEmpty.svelte";

  interface Props {
    sideMode: SideNavMode;
    activeId: string | null;
    activeSession: ConnectedSession | null;
    connectionTabs: WorkspaceTab[];
    activeTabId: string | null;
    activeTab: WorkspaceTab | null;
    currentDatabaseName: string;
    selectedSaved: SavedQuery | null;
    selectedHistory: QueryHistoryEntry | null;
    queryHistory: QueryHistoryEntry[];
    historyLoading: boolean;
    historyError: string | null;
    catalogError: string | null;
    modelError: string | null;
    modelLoading: boolean;
    activeModel: {
      diagram: ErDiagramState;
      title: string;
      summary: string;
      rev: number;
    } | null;
    gridTable: TableMeta | null;
    tableLoading: boolean;
    rowSelectionIndex: number | null;
    rowSyncTick: number;
    baselineEpoch: number;
    inspectorData: {
      tableTitle: string;
      columns: TableMeta["columns"];
      row: Record<string, unknown>;
      rowIndex: number;
    } | null;
    onopeneditor: (sql: string) => void;
    onhistory: (item?: QueryHistoryItem | null) => void;
    onbaselinechange: () => void;
    onselectrow: (index: number) => void;
    onopenmodelretry: () => void;
    oninspectorchange: (column: string, value: unknown) => void;
  }

  let {
    sideMode,
    activeId,
    activeSession,
    connectionTabs,
    activeTabId,
    activeTab,
    currentDatabaseName,
    selectedSaved,
    selectedHistory,
    queryHistory,
    historyLoading,
    historyError,
    catalogError,
    modelError,
    modelLoading,
    activeModel,
    gridTable,
    tableLoading,
    rowSelectionIndex,
    rowSyncTick,
    baselineEpoch,
    inspectorData,
    onopeneditor,
    onhistory,
    onbaselinechange,
    onselectrow,
    onopenmodelretry,
    oninspectorchange,
  }: Props = $props();
</script>

<main class="flex min-w-0 flex-1 flex-col">
  {#if sideMode === "explorer" && activeId}
    <TableTabs
      tabs={connectionTabs}
      {activeTabId}
      onactivate={(id) => workspaceSession.setActiveTab(id)}
      onclose={(id) => workspaceSession.closeTab(id)}
      onopenquery={() => workspaceSession.openNewQuery(activeId)}
    />
  {/if}

  <div class="flex min-h-0 flex-1">
    {#if sideMode === "saved-queries"}
      {#if selectedSaved}
        <QueryPreview
          kind="saved"
          saved={selectedSaved}
          {onopeneditor}
        />
      {:else}
        <div class="flex flex-1 items-center justify-center p-8">
          <EmptyState
            title="Saved queries"
            description="Pick a saved query from the list to preview its SQL."
          />
        </div>
      {/if}
    {:else if sideMode === "history"}
      <div class="flex min-h-0 min-w-0 flex-1 flex-col">
        {#if historyError}
          <div
            class="shrink-0 border-b border-line bg-warning/10 px-3 py-2 text-caption text-warning"
          >
            History load issue: {historyError}
          </div>
        {/if}
        {#if selectedHistory}
          <QueryPreview
            kind="history"
            history={selectedHistory}
            {onopeneditor}
          />
        {:else}
          <div class="flex flex-1 items-center justify-center p-8">
            <EmptyState
              title={historyLoading ? "Loading history…" : "Query history"}
              description={historyLoading
                ? "Fetching recent runs…"
                : queryHistory.length > 0
                  ? "Select a run from the list on the left."
                  : "Run a query from the Query tab (or open a table) — successful and failed runs show up here for every engine."}
            />
          </div>
        {/if}
      </div>
    {:else if activeSession && activeTab?.kind === "query"}
      {#key activeTab.id}
        <QueryCanvas
          connectionId={activeSession.profile.id}
          connectionName={activeSession.profile.name}
          engine={String(activeSession.profile.engine)}
          profileReadOnly={!!activeSession.profile.readOnly}
          engineSessionId={activeSession.engineSessionId}
          database={currentDatabaseName}
          text={workspaceSession.queryDrafts[activeTab.id] ?? ""}
          writeEnabled={workspaceSession.queryWriteMode[activeTab.id] ?? false}
          ontextchange={(v) => workspaceSession.setQueryDraft(activeTab.id, v)}
          onwritechange={(v) =>
            workspaceSession.setQueryWriteMode(activeTab.id, v)}
          {onhistory}
        />
      {/key}
    {:else if activeSession && activeTab?.kind === "model"}
      <div class="relative flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
        {#if modelError}
          <div
            class="shrink-0 border-b border-line bg-danger/10 px-3 py-2 text-caption text-danger"
          >
            {modelError}
            <button
              type="button"
              class="ml-2 border-none bg-transparent text-accent underline"
              onclick={onopenmodelretry}
            >
              Retry
            </button>
          </div>
        {/if}
        {#if modelLoading && !activeModel}
          <div class="flex flex-1 items-center justify-center p-8">
            <EmptyState
              title="Building diagram…"
              description="Reading tables, columns, and foreign keys from the live database."
            />
          </div>
        {:else if activeModel}
          {#key `${activeTab.id}:${activeModel.rev}`}
            <ErWorkspace
              family={activeModel.diagram.family ||
                familyForEngine(String(activeSession.profile.engine))}
              initialDiagram={activeModel.diagram}
              initialName={activeModel.title}
              statusNote={activeModel.summary}
              embedded={true}
            />
          {/key}
        {:else}
          <div class="flex flex-1 items-center justify-center p-8">
            <EmptyState
              title="ER diagram"
              description="Click “ER diagram” in the explorer to build a live schema map for this database."
            />
          </div>
        {/if}
      </div>
    {:else if activeSession && activeTab?.kind === "table"}
      <div class="relative flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
        {#if catalogError}
          <div
            class="shrink-0 border-b border-line bg-danger/10 px-3 py-2 text-caption text-danger"
          >
            {catalogError}
          </div>
        {/if}
        {#if gridTable}
          {#key `${gridTable.id}:${gridTable.rows.length}:${tableLoading}`}
            <DataGrid
              table={gridTable}
              selectedRowIndex={rowSelectionIndex}
              {rowSyncTick}
              {onbaselinechange}
              onselectrow={onselectrow}
            />
          {/key}
          {#if tableLoading}
            <div
              class="pointer-events-none absolute bottom-4 right-4 z-10 rounded-md border border-line bg-panel px-3 py-1.5 text-caption text-fg-muted shadow"
            >
              Loading live rows…
            </div>
          {/if}
        {:else}
          <div class="flex flex-1 items-center justify-center p-8">
            <EmptyState title="Table" description="Loading table metadata…" />
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
          onchange={oninspectorchange}
        />
      {/if}
    {:else if activeSession}
      <WorkspaceEmpty
        profile={activeSession.profile}
        onnewquery={() => {
          if (activeId) workspaceSession.openNewQuery(activeId);
        }}
      />
    {:else}
      <div class="flex flex-1 items-center justify-center p-8">
        <EmptyState
          title="No session selected"
          description="Pick a connection from the rail or return home to connect."
        />
      </div>
    {/if}
  </div>
</main>
