import { invoke } from "@tauri-apps/api/core";
import type { DataFamily, ErDiagramState } from "$lib/types/er";
import { isTauri } from "./tauri";

export interface ErDiagramRecord {
  id: string;
  name: string;
  family: DataFamily;
  body: ErDiagramState;
  createdAt: string;
  updatedAt: string;
}

export interface SaveErDiagramInput {
  id?: string | null;
  name: string;
  family: DataFamily;
  body: ErDiagramState;
}

function requireTauri() {
  if (!isTauri()) {
    throw new Error("Diagram storage requires Tauri (`npm run tauri dev`).");
  }
}

export async function listDiagrams(
  family?: DataFamily,
): Promise<ErDiagramRecord[]> {
  requireTauri();
  return invoke("list_diagrams", { family: family ?? null });
}

export async function getDiagram(id: string): Promise<ErDiagramRecord> {
  requireTauri();
  return invoke("get_diagram", { id });
}

export async function saveDiagram(
  input: SaveErDiagramInput,
): Promise<ErDiagramRecord> {
  requireTauri();
  return invoke("save_diagram", { input });
}

export async function deleteDiagram(id: string): Promise<void> {
  requireTauri();
  return invoke("delete_diagram", { id });
}
