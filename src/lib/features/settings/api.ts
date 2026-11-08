import { isTauri } from '#core/ipc'
import { invoke } from '@tauri-apps/api/core'
import type { McpAccess, McpHarness, McpStatus, McpWritePolicy } from './mcp-types'
export type { McpAccess, McpHarness, McpStatus, McpWritePolicy }

function requireTauri() {
	if (!isTauri()) {
		throw new Error(
			'MCP settings require the Mystique desktop app.',
		)
	}
}

export async function getMcpStatus(): Promise<McpStatus> {
	requireTauri()
	return invoke('get_mcp_status')
}

export async function setMcpEnabled(enabled: boolean): Promise<McpStatus> {
	requireTauri()
	return invoke('set_mcp_enabled', { enabled })
}

export async function rotateMcpToken(): Promise<McpStatus> {
	requireTauri()
	return invoke('rotate_mcp_token')
}

export async function listMcpHarnesses(): Promise<McpHarness[]> {
	requireTauri()
	return invoke('list_mcp_harnesses')
}

export async function installMcpHarness(id: string): Promise<McpHarness> {
	requireTauri()
	return invoke('install_mcp_harness', { id })
}

export async function getMcpAccess(): Promise<McpAccess> {
	requireTauri()
	return invoke('get_mcp_access')
}

export async function setMcpAccess(
	policy: McpWritePolicy,
	allowlist: string[],
): Promise<McpAccess> {
	requireTauri()
	return invoke('set_mcp_access', { policy, allowlist })
}

export function mcpClientSnippet(status: McpStatus): string {
	const url = status.url?.trim() || 'http://127.0.0.1:17321/mcp'
	const token = status.token?.trim() || 'YOUR_TOKEN'
	return JSON.stringify(
		{
			mcpServers: {
				mystique: {
					url,
					headers: {
						Authorization: `Bearer ${token}`,
					},
				},
			},
		},
		null,
		2,
	)
}

/** The one vault key that seals saved passwords. */
export interface VaultStatus {
	/** `keychain` in release builds, `file` in development builds. */
	source: 'keychain' | 'file' | 'memory'
	/** `idle` until the first saved password is needed. */
	state: 'idle' | 'unlocked' | 'denied'
	message?: string
}

export interface KeychainCleanup {
	/** False when this build does not use the keychain. */
	supported: boolean
	deleted: number
	/** Items that would have needed a keychain prompt to delete. */
	skipped: number
}

/** Never unlocks: safe to call when the pane opens. */
export async function getVaultStatus(): Promise<VaultStatus> {
	requireTauri()
	return invoke('get_vault_status')
}

/** Forget a denied keychain prompt and ask again. */
export async function retryVaultUnlock(): Promise<VaultStatus> {
	requireTauri()
	return invoke('retry_vault_unlock')
}

/** Delete keychain items older versions left behind. Never prompts. */
export async function cleanUpKeychain(): Promise<KeychainCleanup> {
	requireTauri()
	return invoke('purge_keychain_items')
}

/** `mystique-diagnostics-20261109-143005.tar.gz`, in local time. */
export function defaultDiagnosticsName(now = new Date()): string {
	const pad = (n: number) => String(n).padStart(2, '0')
	const date = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`
	const time = `${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`
	return `mystique-diagnostics-${date}-${time}.tar.gz`
}

/**
 * Write a diagnostics bundle (.tar.gz: versions, plugins, redacted settings,
 * connection flags, recent logs; no passwords, hosts or SQL). With no `path`
 * it goes to the Downloads folder. Resolves to the written path.
 */
export async function exportDiagnostics(path?: string): Promise<string> {
	requireTauri()
	return invoke('export_diagnostics', { path: path ?? null })
}
