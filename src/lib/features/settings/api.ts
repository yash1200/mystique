import { isTauri } from '$core/ipc'
import { invoke } from '@tauri-apps/api/core'
import type { McpStatus } from './mcp-types'
export type { McpStatus }

function requireTauri() {
	if (!isTauri()) {
		throw new Error(
			'MCP settings require the desktop app (`npm run tauri dev`).',
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
