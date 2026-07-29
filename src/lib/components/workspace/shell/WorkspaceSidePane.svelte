<script lang="ts">
  import type { ConnectedSession } from "$lib/stores/workspace-session.svelte";
  import type { DatabaseMeta, TableMeta } from "$lib/types/workspace";
  import {
    MOCK_SAVED_QUERIES,
    type QueryHistoryEntry,
  } from "$lib/data/mock-queries";
  import type { SideNavMode } from "../SessionsRail.svelte";
  import ObjectExplorer from "../ObjectExplorer.svelte";
  import QueryListPanel from "../QueryListPanel.svelte";

  interface Props {
    sideMode: SideNavMode;
    activeSession: ConnectedSession | null;
    databases: DatabaseMeta[];
    currentDatabaseName: string;
    activeTableId: string | null;
    refreshing: boolean;
    modelLoading: boolean;
    queryHistory: QueryHistoryEntry[];
    selectedQueryId: string | null;
    selectedHistoryId: string | null;
    onselectquery: (id: string) => void;
    onselecthistory: (id: string) => void;
    onselectdatabase: (name: string) => void;
    onrefresh: () => void;
    oncreatedatabase: (name: string) => void;
    onopen: (table: TableMeta) => void;
    onopenmodel: () => void;
  }

  let {
    sideMode,
    activeSession,
    databases,
    currentDatabaseName,
    activeTableId,
    refreshing,
    modelLoading,
    queryHistory,
    selectedQueryId,
    selectedHistoryId,
    onselectquery,
    onselecthistory,
    onselectdatabase,
    onrefresh,
    oncreatedatabase,
    onopen,
    onopenmodel,
  }: Props = $props();
</script>

{#if sideMode === "saved-queries"}
  <QueryListPanel
    mode="saved-queries"
    savedQueries={MOCK_SAVED_QUERIES}
    history={queryHistory}
    selectedId={selectedQueryId}
    onselect={onselectquery}
  />
{:else if sideMode === "history"}
  <QueryListPanel
    mode="history"
    savedQueries={MOCK_SAVED_QUERIES}
    history={queryHistory}
    selectedId={selectedHistoryId}
    onselect={onselecthistory}
  />
{:else if activeSession}
  <ObjectExplorer
    engine={String(activeSession.profile.engine)}
    {databases}
    selectedDatabase={currentDatabaseName}
    activeTableId={activeTableId}
    {refreshing}
    {modelLoading}
    {onselectdatabase}
    {onrefresh}
    {oncreatedatabase}
    {onopen}
    {onopenmodel}
  />
{:else}
  <aside
    class="flex w-60 shrink-0 flex-col items-center justify-center border-r border-line bg-panel px-4 text-center"
  >
    <p class="text-caption text-fg-muted">Select a session</p>
  </aside>
{/if}
