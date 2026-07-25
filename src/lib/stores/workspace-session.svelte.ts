/**
 * Ephemeral connected-session state for the workspace shell.
 * Survives SPA navigations within the app process (not durable across restarts).
 */
import type { ConnectionProfile } from "$lib/types/connection";
import type { OpenTableTab, SelectedRow } from "$lib/types/workspace";

export type ConnectedSession = {
  profile: ConnectionProfile;
  connectedAt: string;
};

class WorkspaceSessionStore {
  sessions = $state<ConnectedSession[]>([]);
  activeConnectionId = $state<string | null>(null);
  openTabs = $state<OpenTableTab[]>([]);
  activeTabId = $state<string | null>(null);
  selectedRow = $state<SelectedRow | null>(null);

  get activeSession(): ConnectedSession | null {
    return (
      this.sessions.find((s) => s.profile.id === this.activeConnectionId) ??
      null
    );
  }

  /** Register / refresh a live session and focus it. */
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
    this.activeConnectionId = profile.id;
  }

  setActiveConnection(id: string) {
    if (!this.sessions.some((s) => s.profile.id === id)) return;
    this.activeConnectionId = id;
    // Clear row selection when switching connections (tabs stay open)
    if (this.selectedRow && this.selectedRow.connectionId !== id) {
      this.selectedRow = null;
    }
    // Prefer a tab for this connection if any
    const tab = this.openTabs.find((t) => t.connectionId === id);
    if (tab) this.activeTabId = tab.id;
  }

  disconnect(id: string) {
    this.sessions = this.sessions.filter((s) => s.profile.id !== id);
    this.openTabs = this.openTabs.filter((t) => t.connectionId !== id);
    if (this.activeConnectionId === id) {
      this.activeConnectionId = this.sessions[0]?.profile.id ?? null;
      const tab = this.openTabs.find(
        (t) => t.connectionId === this.activeConnectionId,
      );
      this.activeTabId = tab?.id ?? this.openTabs[0]?.id ?? null;
    }
    if (this.selectedRow?.connectionId === id) {
      this.selectedRow = null;
    }
  }

  openTable(connectionId: string, tableId: string, title: string) {
    const id = `${connectionId}::${tableId}`;
    if (!this.openTabs.some((t) => t.id === id)) {
      this.openTabs = [
        ...this.openTabs,
        { id, connectionId, tableId, title },
      ];
    }
    this.activeTabId = id;
    this.activeConnectionId = connectionId;
    this.selectedRow = null;
  }

  closeTab(tabId: string) {
    const idx = this.openTabs.findIndex((t) => t.id === tabId);
    if (idx < 0) return;
    const wasActive = this.activeTabId === tabId;
    this.openTabs = this.openTabs.filter((t) => t.id !== tabId);
    if (wasActive) {
      const next = this.openTabs[Math.max(0, idx - 1)] ?? this.openTabs[0];
      this.activeTabId = next?.id ?? null;
    }
    if (this.selectedRow) {
      const tab = this.openTabs.find((t) => t.id === this.activeTabId);
      if (
        !tab ||
        tab.connectionId !== this.selectedRow.connectionId ||
        tab.tableId !== this.selectedRow.tableId
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
      (this.selectedRow.connectionId !== tab.connectionId ||
        this.selectedRow.tableId !== tab.tableId)
    ) {
      this.selectedRow = null;
    }
  }

  selectRow(connectionId: string, tableId: string, rowIndex: number) {
    this.selectedRow = { connectionId, tableId, rowIndex };
  }

  clearRowSelection() {
    this.selectedRow = null;
  }
}

export const workspaceSession = new WorkspaceSessionStore();
