/**
 * Ephemeral connected-session state for the workspace shell.
 * Survives SPA navigations within the app process (not durable across restarts).
 */
import type { ConnectionProfile } from "$lib/types/connection";
import type { SelectedRow, WorkspaceTab } from "$lib/types/workspace";

export type ConnectedSession = {
  profile: ConnectionProfile;
  connectedAt: string;
};

class WorkspaceSessionStore {
  sessions = $state<ConnectedSession[]>([]);
  activeConnectionId = $state<string | null>(null);
  openTabs = $state<WorkspaceTab[]>([]);
  activeTabId = $state<string | null>(null);
  selectedRow = $state<SelectedRow | null>(null);

  /** Draft SQL/MQL text per query tab id. */
  queryDrafts = $state<Record<string, string>>({});
  /** Allow-writes flag per query tab id. */
  queryWriteMode = $state<Record<string, boolean>>({});

  get activeSession(): ConnectedSession | null {
    return (
      this.sessions.find((s) => s.profile.id === this.activeConnectionId) ??
      null
    );
  }

  get activeTab(): WorkspaceTab | null {
    return this.openTabs.find((t) => t.id === this.activeTabId) ?? null;
  }

  /** Register / refresh a live session and focus it (no tabs opened by default). */
  connect(profile: ConnectionProfile) {
    const now = new Date().toISOString();
    const existing = this.sessions.findIndex((s) => s.profile.id === profile.id);
    const next: ConnectedSession = {
      profile: { ...profile, status: "connected" },
      connectedAt: now,
    };
    if (existing >= 0) {
      this.sessions = this.sessions.map((s, i) => (i === existing ? next : s));
    } else {
      this.sessions = [...this.sessions, next];
    }
    this.setActiveConnection(profile.id);
  }

  /**
   * Ensure a Query canvas tab exists for this connection and focus one.
   * Does not create a second query tab if one already exists.
   */
  openQuery(connectionId: string) {
    const existing = this.openTabs.find(
      (t) => t.connectionId === connectionId && t.kind === "query",
    );
    if (existing) {
      this.activeTabId = existing.id;
      this.activeConnectionId = connectionId;
      this.selectedRow = null;
      return;
    }
    this.openNewQuery(connectionId);
  }

  /** Always open a fresh Query canvas tab (⌘/Ctrl+T, “+ Query”). */
  openNewQuery(connectionId: string) {
    const count = this.openTabs.filter(
      (t) => t.connectionId === connectionId && t.kind === "query",
    ).length;
    const id = `${connectionId}::query::${crypto.randomUUID()}`;
    const title = count === 0 ? "Query" : `Query ${count + 1}`;
    this.openTabs = [
      ...this.openTabs,
      { id, connectionId, kind: "query", title },
    ];
    this.queryDrafts = { ...this.queryDrafts, [id]: "" };
    this.queryWriteMode = { ...this.queryWriteMode, [id]: false };
    this.activeTabId = id;
    this.activeConnectionId = connectionId;
    this.selectedRow = null;
  }

  /** Close the active workspace tab (⌘/Ctrl+W). */
  closeActiveTab() {
    if (!this.activeTabId) return;
    this.closeTab(this.activeTabId);
  }

  setActiveConnection(id: string) {
    if (!this.sessions.some((s) => s.profile.id === id)) return;
    this.activeConnectionId = id;
    if (this.selectedRow && this.selectedRow.connectionId !== id) {
      this.selectedRow = null;
    }
    // Prefer an existing tab for this connection; otherwise show empty workspace.
    const tab =
      this.openTabs.find(
        (t) => t.connectionId === id && t.id === this.activeTabId,
      ) ?? this.openTabs.find((t) => t.connectionId === id);
    this.activeTabId = tab?.id ?? null;
  }

  disconnect(id: string) {
    this.sessions = this.sessions.filter((s) => s.profile.id !== id);
    const removedTabIds = this.openTabs
      .filter((t) => t.connectionId === id)
      .map((t) => t.id);
    this.openTabs = this.openTabs.filter((t) => t.connectionId !== id);
    const drafts = { ...this.queryDrafts };
    const writes = { ...this.queryWriteMode };
    for (const tid of removedTabIds) {
      delete drafts[tid];
      delete writes[tid];
    }
    this.queryDrafts = drafts;
    this.queryWriteMode = writes;

    if (this.activeConnectionId === id) {
      this.activeConnectionId = this.sessions[0]?.profile.id ?? null;
      if (this.activeConnectionId) {
        const tab = this.openTabs.find(
          (t) => t.connectionId === this.activeConnectionId,
        );
        this.activeTabId = tab?.id ?? null;
      } else {
        this.activeTabId = null;
      }
    }
    if (this.selectedRow?.connectionId === id) {
      this.selectedRow = null;
    }
  }

  openTable(connectionId: string, tableId: string, title: string) {
    const id = `${connectionId}::table::${tableId}`;
    if (!this.openTabs.some((t) => t.id === id)) {
      this.openTabs = [
        ...this.openTabs,
        { id, connectionId, kind: "table", tableId, title },
      ];
    }
    this.activeTabId = id;
    this.activeConnectionId = connectionId;
    this.selectedRow = null;
  }

  closeTab(tabId: string) {
    const tab = this.openTabs.find((t) => t.id === tabId);
    if (!tab) return;
    const connectionId = tab.connectionId;
    const idx = this.openTabs.findIndex((t) => t.id === tabId);
    const wasActive = this.activeTabId === tabId;
    this.openTabs = this.openTabs.filter((t) => t.id !== tabId);

    if (tab.kind === "query") {
      const drafts = { ...this.queryDrafts };
      const writes = { ...this.queryWriteMode };
      delete drafts[tabId];
      delete writes[tabId];
      this.queryDrafts = drafts;
      this.queryWriteMode = writes;
    }

    // No auto-reopen — empty workspace is valid until the user opens a tab.
    if (wasActive) {
      const sameConn = this.openTabs.filter((t) => t.connectionId === connectionId);
      const next =
        sameConn[Math.max(0, idx - 1)] ??
        sameConn[0] ??
        this.openTabs[Math.max(0, idx - 1)] ??
        this.openTabs[0];
      this.activeTabId = next?.id ?? null;
      if (next) this.activeConnectionId = next.connectionId;
      // Stay on this connection with no tab selected when all its tabs closed.
      else if (this.sessions.some((s) => s.profile.id === connectionId)) {
        this.activeConnectionId = connectionId;
      }
    }
    if (this.selectedRow) {
      const active = this.openTabs.find((t) => t.id === this.activeTabId);
      if (
        !active ||
        active.kind !== "table" ||
        active.connectionId !== this.selectedRow.connectionId ||
        active.tableId !== this.selectedRow.tableId
      ) {
        this.selectedRow = null;
      }
    }
  }

  setActiveTab(tabId: string) {
    const tab = this.openTabs.find((t) => t.id === tabId);
    if (!tab) return;
    this.activeTabId = tabId;
    this.activeConnectionId = tab.connectionId;
    if (
      this.selectedRow &&
      (tab.kind !== "table" ||
        this.selectedRow.connectionId !== tab.connectionId ||
        this.selectedRow.tableId !== tab.tableId)
    ) {
      this.selectedRow = null;
    }
  }

  setQueryDraft(tabId: string, text: string) {
    this.queryDrafts = { ...this.queryDrafts, [tabId]: text };
  }

  setQueryWriteMode(tabId: string, enabled: boolean) {
    this.queryWriteMode = { ...this.queryWriteMode, [tabId]: enabled };
  }

  selectRow(connectionId: string, tableId: string, rowIndex: number) {
    this.selectedRow = { connectionId, tableId, rowIndex };
  }

  clearRowSelection() {
    this.selectedRow = null;
  }
}

export const workspaceSession = new WorkspaceSessionStore();
