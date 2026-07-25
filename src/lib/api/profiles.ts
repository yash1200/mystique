import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionProfile,
  SaveConnectionInput,
} from "$lib/types/connection";
import { asColorId, asEnvironment } from "$lib/types/connection";
import { isTauri } from "./tauri";

const LS_KEY = "mystique-db.profiles.v1";

/** Normalize backend / local payload into a typed UI profile. */
export function normalizeProfile(raw: ConnectionProfile): ConnectionProfile {
  return {
    ...raw,
    colorId: asColorId(String(raw.colorId ?? "slate")),
    environment: asEnvironment(String(raw.environment ?? "local")),
    pluginConfig: (raw.pluginConfig ?? {}) as ConnectionProfile["pluginConfig"],
    notes: raw.notes ?? null,
    lastConnectedAt: raw.lastConnectedAt ?? null,
    status: raw.status ?? "disconnected",
  };
}

function nowIso(): string {
  return new Date().toISOString();
}

function readLocal(): ConnectionProfile[] {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as ConnectionProfile[];
    if (!Array.isArray(parsed)) return [];
    return parsed.map(normalizeProfile);
  } catch {
    return [];
  }
}

function writeLocal(rows: ConnectionProfile[]) {
  localStorage.setItem(LS_KEY, JSON.stringify(rows));
}

function localSave(input: SaveConnectionInput): ConnectionProfile {
  const rows = readLocal();
  const ts = nowIso();
  const id =
    (input.id && String(input.id).trim()) ||
    (typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID()
      : `local-${Date.now()}`);

  const existing = rows.find((r) => r.id === id);
  const passwordSet =
    !!input.secretRef ||
    (!input.clearSecret && (existing?.passwordSet ?? false));

  const profile: ConnectionProfile = {
    id,
    name: input.name,
    engine: input.engine,
    colorId: asColorId(input.colorId),
    environment: asEnvironment(input.environment),
    readOnly: input.readOnly,
    notes: input.notes ?? null,
    pluginConfig: input.pluginConfig ?? {},
    passwordSet,
    lastConnectedAt: existing?.lastConnectedAt ?? null,
    createdAt: existing?.createdAt ?? ts,
    updatedAt: ts,
    status: existing?.status ?? "disconnected",
  };

  const next = existing
    ? rows.map((r) => (r.id === id ? profile : r))
    : [...rows, profile];
  writeLocal(next);
  return normalizeProfile(profile);
}

export async function listProfiles(): Promise<ConnectionProfile[]> {
  if (!isTauri()) {
    return readLocal();
  }
  const rows = await invoke<ConnectionProfile[]>("list_profiles");
  return rows.map(normalizeProfile);
}

export async function getProfile(id: string): Promise<ConnectionProfile> {
  if (!isTauri()) {
    const row = readLocal().find((r) => r.id === id);
    if (!row) throw new Error(`Profile not found: ${id}`);
    return row;
  }
  const row = await invoke<ConnectionProfile>("get_profile", { id });
  return normalizeProfile(row);
}

export async function saveProfile(
  input: SaveConnectionInput,
): Promise<ConnectionProfile> {
  if (!isTauri()) {
    return localSave(input);
  }
  const row = await invoke<ConnectionProfile>("save_profile", { input });
  return normalizeProfile(row);
}

export async function deleteProfile(id: string): Promise<void> {
  if (!isTauri()) {
    writeLocal(readLocal().filter((r) => r.id !== id));
    return;
  }
  await invoke("delete_profile", { id });
}

export async function touchProfileConnected(
  id: string,
): Promise<ConnectionProfile> {
  if (!isTauri()) {
    const rows = readLocal();
    const ts = nowIso();
    const next = rows.map((r) =>
      r.id === id
        ? { ...r, lastConnectedAt: ts, updatedAt: ts, status: "connected" as const }
        : r,
    );
    writeLocal(next);
    const row = next.find((r) => r.id === id);
    if (!row) throw new Error(`Profile not found: ${id}`);
    return normalizeProfile(row);
  }
  const row = await invoke<ConnectionProfile>("touch_profile_connected", {
    id,
  });
  return normalizeProfile(row);
}
