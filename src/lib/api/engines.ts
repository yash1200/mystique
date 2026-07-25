/**
 * Engine / connection-pool API (Tauri IPC).
 * Mirrors mystique-core ConnectionManager surface.
 */
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tauri";

export interface PoolConfig {
  maxSize?: number;
  minIdle?: number;
  acquireTimeoutMs?: number;
  idleTimeoutSecs?: number;
}

export interface OpenSessionRequest {
  engine: string;
  pluginConfig: Record<string, unknown>;
  password?: string | null;
  pool?: PoolConfig;
  sessionId?: string | null;
}

export interface EngineDescriptor {
  id: string;
  displayName: string;
  family: string;
  queryLanguage: string;
  available: boolean;
  supportsPooling: boolean;
  description: string;
}

export interface PoolStatus {
  engine: string;
  maxSize: number;
  active: number;
  idle: number;
  size: number;
}

export interface SessionInfo {
  sessionId: string;
  engine: string;
  displayName: string;
  capabilities: Record<string, unknown>;
  pool?: PoolStatus | null;
  openedAt: string;
}

export interface TestResult {
  ok: boolean;
  engine: string;
  serverVersion?: string | null;
  latencyMs: number;
  message: string;
}

function requireTauri() {
  if (!isTauri()) {
    throw new Error("Engine pool API requires Tauri (`yarn tauri dev`).");
  }
}

export async function listEngines(): Promise<EngineDescriptor[]> {
  requireTauri();
  return invoke("list_engines");
}

export async function testEngineConnection(
  request: OpenSessionRequest,
): Promise<TestResult> {
  requireTauri();
  return invoke("test_engine_connection", { request });
}

export async function openEngineSession(
  request: OpenSessionRequest,
): Promise<SessionInfo> {
  requireTauri();
  return invoke("open_engine_session", { request });
}

export async function closeEngineSession(sessionId: string): Promise<void> {
  requireTauri();
  return invoke("close_engine_session", { sessionId });
}

export async function pingEngineSession(sessionId: string): Promise<void> {
  requireTauri();
  return invoke("ping_engine_session", { sessionId });
}

export async function engineSessionPoolStatus(
  sessionId: string,
): Promise<PoolStatus | null> {
  requireTauri();
  return invoke("engine_session_pool_status", { sessionId });
}

export async function listEngineSessions(): Promise<SessionInfo[]> {
  requireTauri();
  return invoke("list_engine_sessions");
}

export async function getEngineSession(
  sessionId: string,
): Promise<SessionInfo | null> {
  requireTauri();
  return invoke("get_engine_session", { sessionId });
}
