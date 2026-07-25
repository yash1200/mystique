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
  secretRef?: string | null;
  clearSecret?: boolean;
}

/** Form draft used by NewConnectionPanel (relational engines for now). */
export interface ConnectionFormDraft {
  name: string;
  engine: EngineId;
  colorId: ConnectionColorId;
  host: string;
  port: number;
  database: string;
  username: string;
  password: string;
  ssl: boolean;
  readOnly: boolean;
  environment: Environment;
  notes: string;
}

export interface EngineOption {
  id: EngineId;
  name: string;
  description: string;
  available: boolean;
  queryLanguage: string;
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

export const ENGINE_OPTIONS: EngineOption[] = [
  {
    id: "postgres",
    name: "PostgreSQL",
    description: "Full support — explorer, query, model view",
    available: true,
    queryLanguage: "SQL",
  },
  {
    id: "sqlite",
    name: "SQLite",
    description: "Coming soon — same relational workspace",
    available: false,
    queryLanguage: "SQL",
  },
  {
    id: "mysql",
    name: "MySQL / MariaDB",
    description: "Coming soon",
    available: false,
    queryLanguage: "SQL",
  },
  {
    id: "mongodb",
    name: "MongoDB",
    description: "Coming soon — document family",
    available: false,
    queryLanguage: "MQL",
  },
];

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
  return {
    name: profile.name,
    engine: profile.engine,
    colorId: asColorId(profile.colorId),
    host: configStr(c, "host", "127.0.0.1"),
    port: configNum(c, "port", 5432),
    database: configStr(c, "database"),
    username: configStr(c, "username"),
    password: "",
    ssl: configBool(c, "ssl", false) || configStr(c, "sslMode") === "require",
    readOnly: profile.readOnly,
    environment: asEnvironment(profile.environment),
    notes: profile.notes ?? "",
  };
}

/** Build SaveConnectionInput from the relational form draft. */
export function saveInputFromDraft(
  draft: ConnectionFormDraft,
  options?: { id?: string; existingPasswordSet?: boolean },
): SaveConnectionInput {
  const pluginConfig: PluginConfig = {
    host: draft.host.trim(),
    port: Number(draft.port) || 0,
    database: draft.database.trim(),
    username: draft.username.trim(),
    ssl: draft.ssl,
    sslMode: draft.ssl ? "require" : "disable",
  };

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

  // Keychain not wired yet: non-empty secretRef marks passwordSet for UI.
  // When keychain lands, store the real secret under this ref.
  if (draft.password.length > 0) {
    const idPart = options?.id ?? "new";
    input.secretRef = `mystique-db/connection/${idPart}`;
  }

  return input;
}
