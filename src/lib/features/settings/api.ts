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
