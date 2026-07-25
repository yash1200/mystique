export type EngineId = "postgres" | "sqlite" | "mysql" | "mongodb" | string;

export type ConnectionColorId =
  | "rose"
  | "orange"
  | "amber"
  | "lime"
  | "green"
  | "teal"
  | "cyan"
  | "blue"
  | "indigo"
  | "violet"
  | "magenta"
  | "slate";

export type ConnectionStatus = "connected" | "disconnected" | "error";

export type Environment = "local" | "staging" | "production";

/** Engine-specific fields stored as JSON in SQLite `plugin_config`. */
export type PluginConfig = Record<string, unknown>;

/**
 * Connection profile — matches mystique-core `ConnectionProfile` (camelCase).
 * `status` is UI-only (not persisted).
 *
 * Password is returned plaintext for now (local app DB). Keychain later.
 */
export interface ConnectionProfile {
  id: string;
  name: string;
  engine: EngineId;
  colorId: ConnectionColorId;
  environment: Environment;
  readOnly: boolean;
  notes?: string | null;
  pluginConfig: PluginConfig;
  passwordSet: boolean;
  /** Plaintext password when stored (temporary — not keychain). */
  password?: string | null;
  lastConnectedAt: string | null;
  createdAt: string;
  updatedAt: string;
  /** Ephemeral UI state; defaults to disconnected. */
  status?: ConnectionStatus;
}

/** Payload for create/update — matches backend `SaveConnectionInput`. */
export interface SaveConnectionInput {
  id?: string | null;
  name: string;
  engine: string;
  colorId: string;
  environment: string;
  readOnly: boolean;
  notes?: string | null;
  pluginConfig: PluginConfig;
  /** Plaintext password to store. Blank on edit keeps the existing secret. */
  password?: string | null;
  /** When true, clear the stored password. */
  clearSecret?: boolean;
  /** @deprecated Prefer `password`. Kept for IPC compatibility. */
  secretRef?: string | null;
}

/** Form draft used by NewConnectionPanel (engine-specific fields optional). */
export interface ConnectionFormDraft {
  name: string;
  engine: EngineId;
  colorId: ConnectionColorId;
  host: string;
  port: number;
  database: string;
  /** SQLite file path (when engine === sqlite). */
  path: string;
  /** MongoDB connection URI (required for mongodb engine). */
  uri: string;
  username: string;
  password: string;
  ssl: boolean;
  readOnly: boolean;
  environment: Environment;
  notes: string;
}

export type EngineFamily = "relational" | "document";

export interface EngineOption {
  id: EngineId;
  name: string;
  /** Short card blurb for the engine picker. */
  description: string;
  /** Whether the engine is selectable in the UI (plugin may still be FE-only). */
  available: boolean;
  queryLanguage: string;
  family: EngineFamily;
  /** Mono badge mark in the picker / EngineBadge. */
  mark: string;
  /** Default port for network engines (0 = file / N/A). */
  defaultPort: number;
  /** Model view kind label for empty states / copy. */
  modelViewLabel: string;
}

export const CONNECTION_COLORS: {
  id: ConnectionColorId;
  name: string;
  hex: string;
}[] = [
  { id: "rose", name: "Rose", hex: "#e86a7a" },
  { id: "orange", name: "Orange", hex: "#e8956a" },
  { id: "amber", name: "Amber", hex: "#e6b84d" },
  { id: "lime", name: "Lime", hex: "#a8d46a" },
  { id: "green", name: "Green", hex: "#5ecf8e" },
  { id: "teal", name: "Teal", hex: "#4ecdc4" },
  { id: "cyan", name: "Cyan", hex: "#4eb8e8" },
  { id: "blue", name: "Blue", hex: "#5b8def" },
  { id: "indigo", name: "Indigo", hex: "#7b7ef0" },
  { id: "violet", name: "Violet", hex: "#9b6de8" },
  { id: "magenta", name: "Magenta", hex: "#d46ad8" },
  { id: "slate", name: "Slate", hex: "#8b95a8" },
];

export function colorHex(id: string): string {
  return CONNECTION_COLORS.find((c) => c.id === id)?.hex ?? "#8b95a8";
}

export function asColorId(id: string): ConnectionColorId {
  return (CONNECTION_COLORS.find((c) => c.id === id)?.id ?? "slate") as ConnectionColorId;
}

export function asEnvironment(env: string): Environment {
  if (env === "staging" || env === "production" || env === "local") return env;
  return "local";
}

/**
 * Engines offered on the Home “choose database” step.
 * V1 product focus: PostgreSQL, MongoDB, and SQLite (SQL) only.
 */
export const ENGINE_OPTIONS: EngineOption[] = [
  {
    id: "postgres",
    name: "PostgreSQL",
    description:
      "Relational studio — schemas, tables, SQL, and classic ER Model view from foreign keys.",
    available: true,
    queryLanguage: "SQL",
    family: "relational",
    mark: "Pg",
    defaultPort: 5432,
    modelViewLabel: "ER diagram",
  },
  {
    id: "mongodb",
    name: "MongoDB",
    description:
      "Document studio — databases, collections, find/aggregation, and collection Model map.",
    available: true,
    queryLanguage: "MQL",
    family: "document",
    mark: "Mo",
    defaultPort: 27017,
    modelViewLabel: "Collection map",
  },
  {
    id: "sqlite",
    name: "SQLite",
    description:
      "Local SQL file — same relational workspace as Postgres for offline and embedded DBs.",
    available: true,
    queryLanguage: "SQL",
    family: "relational",
    mark: "SQL",
    defaultPort: 0,
    modelViewLabel: "ER diagram",
  },
];

/** Engines shown on the first-step picker (available only). */
export function selectableEngines(): EngineOption[] {
  return ENGINE_OPTIONS.filter((e) => e.available);
}

export function emptyDraft(engine: EngineId = "postgres"): ConnectionFormDraft {
  const opt = ENGINE_OPTIONS.find((e) => e.id === engine);
  const port = opt?.defaultPort ?? 5432;
  return {
    name: "",
    engine,
    colorId: "teal",
    host: engine === "sqlite" || engine === "mongodb" ? "" : "127.0.0.1",
    port: engine === "mongodb" ? 27017 : port || 5432,
    // Postgres: Docker images use db/user `postgres`. Homebrew macOS often
    // needs your OS username instead — the form shows these so they are editable.
    // MongoDB uses a connection URI only (no discrete host/user fields).
    database: engine === "mongodb" || engine === "sqlite" ? "" : "postgres",
    path: "",
    uri: engine === "mongodb" ? "mongodb://127.0.0.1:27017" : "",
    username: engine === "mongodb" || engine === "sqlite" ? "" : "postgres",
    password: "",
    ssl: false,
    readOnly: true,
    environment: "local",
    notes: "",
  };
}

export function engineLabel(engine: string): string {
  return ENGINE_OPTIONS.find((e) => e.id === engine)?.name ?? engine;
}

export function configStr(config: PluginConfig, key: string, fallback = ""): string {
  const v = config[key];
  if (v == null) return fallback;
  return String(v);
}

export function configNum(config: PluginConfig, key: string, fallback = 0): number {
  const v = config[key];
  if (typeof v === "number" && !Number.isNaN(v)) return v;
  if (typeof v === "string" && v.trim() !== "") {
    const n = Number(v);
    if (!Number.isNaN(n)) return n;
  }
  return fallback;
}

export function configBool(config: PluginConfig, key: string, fallback = false): boolean {
  const v = config[key];
  if (typeof v === "boolean") return v;
  return fallback;
}

/** Short endpoint label for the sidebar (host:port, path, or uri). */
export function connectionEndpoint(profile: ConnectionProfile): string {
  const c = profile.pluginConfig ?? {};
  const path = configStr(c, "path");
  if (path) return path;
  const uri = configStr(c, "uri");
  if (uri) return uri;
  const host = configStr(c, "host");
  const port = configNum(c, "port", 0);
  if (host && port) return `${host}:${port}`;
  if (host) return host;
  return "—";
}

export function draftFromProfile(profile: ConnectionProfile): ConnectionFormDraft {
  const c = profile.pluginConfig ?? {};
  const eng = String(profile.engine);
  const opt = ENGINE_OPTIONS.find((e) => e.id === eng);
  return {
    name: profile.name,
    engine: profile.engine,
    colorId: asColorId(profile.colorId),
    host: configStr(c, "host", eng === "sqlite" ? "" : "127.0.0.1"),
    port: configNum(c, "port", opt?.defaultPort || 5432),
    database: configStr(c, "database"),
    path: configStr(c, "path"),
    uri: configStr(c, "uri"),
    username: configStr(c, "username"),
    password: profile.password ?? "",
    ssl: configBool(c, "ssl", false) || configStr(c, "sslMode") === "require",
    readOnly: profile.readOnly,
    environment: asEnvironment(profile.environment),
    notes: profile.notes ?? "",
  };
}

/** Build engine `pluginConfig` JSON from a form draft (shared by save + test). */
export function pluginConfigFromDraft(draft: ConnectionFormDraft): PluginConfig {
  if (draft.engine === "sqlite") {
    return {
      path: draft.path.trim() || draft.database.trim(),
    };
  }
  if (draft.engine === "mongodb") {
    return {
      uri: draft.uri.trim(),
    };
  }
  // postgres (default relational network)
  return {
    host: draft.host.trim() || "127.0.0.1",
    port: Number(draft.port) || 5432,
    database: draft.database.trim() || "postgres",
    // Prefer explicit user; empty string is omitted so backend default applies
    username: draft.username.trim() || "postgres",
    ssl: draft.ssl,
    sslMode: draft.ssl ? "require" : "disable",
  };
}

/** Build SaveConnectionInput from the form draft (engine-aware plugin_config). */
export function saveInputFromDraft(
  draft: ConnectionFormDraft,
  options?: { id?: string; existingPasswordSet?: boolean; clearPassword?: boolean },
): SaveConnectionInput {
  const pluginConfig = pluginConfigFromDraft(draft);

  const input: SaveConnectionInput = {
    id: options?.id ?? null,
    name: draft.name.trim(),
    engine: String(draft.engine),
    colorId: draft.colorId,
    environment: draft.environment,
    readOnly: draft.readOnly,
    notes: draft.notes.trim() || null,
    pluginConfig,
  };

  // Plaintext password in local SQLite (secret_ref column). Blank on edit keeps existing.
  if (options?.clearPassword) {
    input.clearSecret = true;
  } else if (draft.password.length > 0) {
    input.password = draft.password;
  }

  return input;
}
