/**
 * Persisted open workspace tabs (query / table / model) per connection.
 */
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tauri";
import type { WorkspaceTab, WorkspaceTabKind } from "$lib/types/workspace";

export type WorkspaceTabRecord = {
  id: string;
  connectionId: string;
  kind: string;
  title: string;
  sortOrder: number;
  isActive: boolean;
  savedQueryId?: string | null;
  body?: string | null;
  language?: string | null;
  writeEnabled: boolean;
  tableId?: string | null;
  modelDatabase?: string | null;
  isDirty: boolean;
  createdAt: string;
  updatedAt: string;
};

export type SaveWorkspaceTabInput = {
  id: string;
  connectionId: string;
  kind: string;
  title: string;
  sortOrder?: number;
  isActive?: boolean;
  savedQueryId?: string | null;
  body?: string | null;
  language?: string | null;
  writeEnabled?: boolean;
  tableId?: string | null;
  modelDatabase?: string | null;
  isDirty?: boolean;
};

export type ReplaceWorkspaceTabsInput = {
  connectionId: string;
  tabs: SaveWorkspaceTabInput[];
  activeTabId?: string | null;
};

function requireTauri() {
  if (!isTauri()) {
    throw new Error("Workspace tabs require Tauri (`npm run tauri dev`).");
  }
}

export async function listWorkspaceTabs(
  connectionId: string,
): Promise<WorkspaceTabRecord[]> {
  if (!isTauri()) return [];
  return invoke("list_workspace_tabs", { connectionId });
}

export async function replaceWorkspaceTabs(
  input: ReplaceWorkspaceTabsInput,
): Promise<WorkspaceTabRecord[]> {
  requireTauri();
  return invoke("replace_workspace_tabs", { input });
}

export async function deleteWorkspaceTab(id: string): Promise<void> {
  if (!isTauri()) return;
  return invoke("delete_workspace_tab", { id });
}

export function recordToWorkspaceTab(r: WorkspaceTabRecord): WorkspaceTab {
  const kind = r.kind as WorkspaceTabKind;
  return {
    id: r.id,
    connectionId: r.connectionId,
    kind,
    title: r.title,
    tableId: r.tableId ?? undefined,
    modelDatabase: r.modelDatabase ?? undefined,
    savedQueryId: r.savedQueryId ?? null,
    dirty: !!r.isDirty,
  };
}

export function workspaceTabToSaveInput(
  tab: WorkspaceTab,
  opts: {
    body?: string;
    writeEnabled?: boolean;
    language?: string | null;
    sortOrder: number;
    isActive: boolean;
  },
): SaveWorkspaceTabInput {
  return {
    id: tab.id,
    connectionId: tab.connectionId,
    kind: tab.kind,
    title: tab.title,
    sortOrder: opts.sortOrder,
    isActive: opts.isActive,
    savedQueryId: tab.savedQueryId ?? null,
    body: tab.kind === "query" ? (opts.body ?? "") : null,
    language: opts.language ?? null,
    writeEnabled: opts.writeEnabled ?? false,
    tableId: tab.tableId ?? null,
    modelDatabase: tab.modelDatabase ?? null,
    isDirty: !!tab.dirty,
  };
}
