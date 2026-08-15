import { invoke } from '@tauri-apps/api/core'
import { isTauri } from './tauri'

export interface McpStatus {
	enabled: boolean
	running: boolean
	port?: number | null
	url?: string | null
	token?: string | null
}

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
				'mystique-db': {
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
