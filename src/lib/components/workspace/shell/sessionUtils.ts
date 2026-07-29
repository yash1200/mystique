import {
  configStr,
  databaseFromMongoUri,
  pluginConfigForSession,
  type ConnectionProfile,
} from "$lib/types/connection";

export function preferredDbForProfile(profile: {
  engine: string;
  pluginConfig?: Record<string, unknown>;
}): string | null {
  try {
    const cfg = pluginConfigForSession(profile as ConnectionProfile);
    const explicit = configStr(cfg, "database").trim();
    if (explicit) return explicit;
    if (String(profile.engine).toLowerCase() === "mongodb") {
      return databaseFromMongoUri(configStr(cfg, "uri"));
    }
  } catch {
    /* fall through */
  }
  const cfg = profile.pluginConfig ?? {};
  const explicit = configStr(cfg, "database").trim();
  if (explicit) return explicit;
  if (String(profile.engine).toLowerCase() === "mongodb") {
    return databaseFromMongoUri(configStr(cfg, "uri"));
  }
  return null;
}

export function isSessionMissingError(err: unknown): boolean {
  const msg = err instanceof Error ? err.message : String(err);
  return /session not found/i.test(msg);
}

export function modelCacheKey(connectionId: string, database: string): string {
  return `${connectionId}::${database.trim() || "_"}`;
}
