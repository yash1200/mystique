import {
  ENGINE_OPTIONS,
  type ConnectionFormDraft,
  type EngineId,
} from "$lib/types/connection";

/** Client-side checks before calling Test connection. */
export function validateDraftForTest(
  draft: ConnectionFormDraft,
): string | null {
  if (draft.engine === "sqlite") {
    if (!draft.path.trim()) return "SQLite database file path is required.";
    return null;
  }
  if (draft.engine === "mongodb") {
    if (!draft.uri.trim()) {
      return "MongoDB connection URI is required.";
    }
    return null;
  }
  // postgres / relational network engines
  if (!draft.host.trim() || !draft.database.trim()) {
    return "Host and database are required.";
  }
  if (!draft.username.trim()) {
    return "Username is required (Docker: postgres · Homebrew: your macOS login).";
  }
  return null;
}

/** Client-side checks before save / update. */
export function validateDraftForSave(
  draft: ConnectionFormDraft,
  options?: { isEdit?: boolean },
): string | null {
  if (!draft.name.trim()) {
    return "Give this connection a name.";
  }

  const selected = ENGINE_OPTIONS.find((e) => e.id === draft.engine);
  if (!selected?.available && !options?.isEdit) {
    return "Selected engine is not available yet.";
  }

  if (draft.engine === "sqlite") {
    if (!draft.path.trim()) return "SQLite database file path is required.";
    return null;
  }
  if (draft.engine === "mongodb") {
    if (!draft.uri.trim()) {
      return "MongoDB connection URI is required.";
    }
    return null;
  }
  if (!draft.host.trim() || !draft.database.trim()) {
    return "Host and database are required.";
  }
  return null;
}

export function namePlaceholder(engine: EngineId): string {
  if (engine === "mongodb") return "e.g. local-mongo";
  if (engine === "sqlite") return "e.g. app-dev.sqlite";
  return "e.g. local-pg";
}
