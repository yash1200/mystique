import type { ConnectionProfile } from "$lib/types/connection";

/** Dev-only fixtures (browser preview). Prefer Tauri + SQLite when available. */
export const MOCK_CONNECTIONS: ConnectionProfile[] = [
  {
    id: "conn-local-pg",
    name: "local-pg",
    engine: "postgres",
    colorId: "teal",
    environment: "local",
    readOnly: false,
    notes: "Docker Compose Postgres on the laptop.",
    pluginConfig: {
      host: "127.0.0.1",
      port: 5432,
      database: "mystique_dev",
      username: "postgres",
      ssl: false,
    },
    passwordSet: true,
    lastConnectedAt: "2026-07-24T18:22:00Z",
    createdAt: "2026-07-20T10:00:00Z",
    updatedAt: "2026-07-24T18:22:00Z",
    status: "disconnected",
  },
  {
    id: "conn-staging",
    name: "staging-analytics",
    engine: "postgres",
    colorId: "violet",
    environment: "staging",
    readOnly: true,
    notes: "Read-only staging replica for reports.",
    pluginConfig: {
      host: "pg-staging.internal",
      port: 5432,
      database: "analytics",
      username: "readonly_app",
      ssl: true,
      sslMode: "require",
    },
    passwordSet: true,
    lastConnectedAt: "2026-07-23T09:10:00Z",
    createdAt: "2026-07-20T10:00:00Z",
    updatedAt: "2026-07-23T09:10:00Z",
    status: "disconnected",
  },
];
