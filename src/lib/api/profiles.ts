import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionProfile,
  SaveConnectionInput,
} from "$lib/types/connection";
import { asColorId, asEnvironment } from "$lib/types/connection";
import { isTauri } from "./tauri";

/** Normalize backend payload into a typed UI profile. */
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

export async function listProfiles(): Promise<ConnectionProfile[]> {
  if (!isTauri()) {
    throw new Error("Not running in Tauri — open with `yarn tauri dev`.");
  }
  const rows = await invoke<ConnectionProfile[]>("list_profiles");
  return rows.map(normalizeProfile);
}

export async function getProfile(id: string): Promise<ConnectionProfile> {
  if (!isTauri()) throw new Error("Not running in Tauri.");
  const row = await invoke<ConnectionProfile>("get_profile", { id });
  return normalizeProfile(row);
}

export async function saveProfile(
  input: SaveConnectionInput,
): Promise<ConnectionProfile> {
  if (!isTauri()) throw new Error("Not running in Tauri.");
  const row = await invoke<ConnectionProfile>("save_profile", { input });
  return normalizeProfile(row);
}

export async function deleteProfile(id: string): Promise<void> {
  if (!isTauri()) throw new Error("Not running in Tauri.");
  await invoke("delete_profile", { id });
}

export async function touchProfileConnected(
  id: string,
): Promise<ConnectionProfile> {
  if (!isTauri()) throw new Error("Not running in Tauri.");
  const row = await invoke<ConnectionProfile>("touch_profile_connected", {
    id,
  });
  return normalizeProfile(row);
}
