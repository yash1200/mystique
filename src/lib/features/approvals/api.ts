import type { ToolApprovalRequest } from '#core/contracts'
import { invoke } from '#core/ipc'

export type { ToolApprovalRequest }

/** Emitted by the backend when a request is queued or ends. */
export const TOOL_APPROVALS_CHANGED = 'tool-approvals-changed'

/** Agent and MCP writes waiting for the user, oldest first. */
export async function listToolApprovals(): Promise<ToolApprovalRequest[]> {
	return invoke('list_tool_approvals')
}

/** Approve once (that exact statement, one run) or deny. */
export async function decideToolApproval(
	id: string,
	approve: boolean,
): Promise<void> {
	return invoke('decide_tool_approval', { id, approve })
}

export async function listenToolApprovals(
	onChange: () => void,
): Promise<() => void> {
	const { listen } = await import('@tauri-apps/api/event')
	return listen(TOOL_APPROVALS_CHANGED, () => onChange())
}

/** Whole seconds until the request is denied; never negative. */
export function secondsLeft(expiresAt: string, now: number): number {
	const end = Date.parse(expiresAt)
	if (Number.isNaN(end)) return 0
	return Math.max(0, Math.ceil((end - now) / 1000))
}

export function callerLabel(caller: string): string {
	return caller === 'agent' ? 'Built-in agent' : 'MCP client'
}

/** "write" → what the statement needs, for the dock. */
export function needLabel(needed: string, granted: string): string {
	const names: Record<string, string> = {
		none: 'None',
		read: 'Read',
		write: 'Write',
		full: 'Full',
	}
	return `Needs ${names[needed] ?? needed}; has ${names[granted] ?? granted}`
}
