export type EngineId = "postgres" | "sqlite" | "mysql" | "mongodb";

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

export interface ConnectionProfile {
  id: string;
  name: string;
  engine: EngineId;
  colorId: ConnectionColorId;
  host: string;
  port: number;
  database: string;
  username: string;
  /** UI-only: never a real secret */
  passwordSet: boolean;
  ssl: boolean;
  readOnly: boolean;
  environment: Environment;
  status: ConnectionStatus;
  lastConnectedAt: string | null;
  notes?: string;
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

export function colorHex(id: ConnectionColorId): string {
  return CONNECTION_COLORS.find((c) => c.id === id)?.hex ?? "#8b95a8";
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

export function engineLabel(engine: EngineId): string {
  return ENGINE_OPTIONS.find((e) => e.id === engine)?.name ?? engine;
}
