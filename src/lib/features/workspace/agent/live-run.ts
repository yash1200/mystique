/**
 * The in-flight reply, built from the streamed `AgentEvent`s. Pure: the
 * chat store holds one of these per live run and replaces it per event.
 * Tool cards render generically by tool name; the full result is on the
 * persisted tool message, loaded when the run ends.
 */
import type { AgentErrorKind, AgentEvent, AgentTokens } from '#core/contracts'

export type LiveToolStatus = 'running' | 'waiting' | 'ok' | 'error'

export type LiveTool = {
	id: string
	name: string
	args: unknown
	status: LiveToolStatus
	summary: string
	/** Set while the call waits in the approval dock. */
	approvalSql?: string
	messageId?: string
}

/** Text and tool calls in the order they streamed. */
export type LivePart =
	| { kind: 'text'; text: string }
	| { kind: 'tool'; tool: LiveTool }

export type LiveRun = {
	/** Client id: events of an older run are ignored. */
	key: string
	connectionId: string
	/** Known once `send_agent_message` returns. */
	chatId: string | null
	parts: LivePart[]
	reasoning: string
	/** Messages dropped from the model's context so far. */
	compacted: number
	exhausted: number | null
	done: { messageId: string; tokens: AgentTokens } | null
	error: { kind: AgentErrorKind; message: string } | null
	/** Stop was pressed before the chat id was known. */
	stopRequested: boolean
}

export function newLiveRun(key: string, connectionId: string): LiveRun {
	return {
		key,
		connectionId,
		chatId: null,
		parts: [],
		reasoning: '',
		compacted: 0,
		exhausted: null,
		done: null,
		error: null,
		stopRequested: false,
	}
}

export function isFinished(run: LiveRun): boolean {
	return run.done !== null || run.error !== null
}

function updateTool(
	parts: LivePart[],
	id: string,
	patch: (tool: LiveTool) => LiveTool,
): LivePart[] {
	return parts.map((part) =>
		part.kind === 'tool' && part.tool.id === id
			? { kind: 'tool', tool: patch(part.tool) }
			: part,
	)
}

/** The run after `event`. Events after the terminal one are ignored. */
export function applyAgentEvent(run: LiveRun, event: AgentEvent): LiveRun {
	if (isFinished(run)) return run
	switch (event.type) {
		case 'textDelta': {
			const last = run.parts.at(-1)
			const parts =
				last?.kind === 'text'
					? [...run.parts.slice(0, -1), { kind: 'text' as const, text: last.text + event.text }]
					: [...run.parts, { kind: 'text' as const, text: event.text.replace(/^\n+/, '') }]
			return { ...run, parts }
		}
		case 'reasoningDelta':
			return { ...run, reasoning: run.reasoning + event.text }
		case 'toolCallStart':
			return {
				...run,
				parts: [
					...run.parts,
					{
						kind: 'tool',
						tool: {
							id: event.id,
							name: event.name,
							args: event.args,
							status: 'running',
							summary: '',
						},
					},
				],
			}
		case 'toolApprovalPending':
			return {
				...run,
				parts: updateTool(run.parts, event.id, (tool) => ({
					...tool,
					status: 'waiting',
					approvalSql: event.sql,
				})),
			}
		case 'toolCallEnd':
			return {
				...run,
				parts: updateTool(run.parts, event.id, (tool) => ({
					...tool,
					status: event.ok ? 'ok' : 'error',
					summary: event.summary,
					approvalSql: undefined,
					messageId: event.messageId,
				})),
			}
		case 'contextCompacted':
			return { ...run, compacted: run.compacted + event.droppedMessages }
		case 'exhausted':
			return { ...run, exhausted: event.turns }
		case 'done':
			return {
				...run,
				done: { messageId: event.messageId, tokens: event.tokens },
			}
		case 'error':
			return { ...run, error: { kind: event.kind, message: event.message } }
	}
}

/** Compact token count for the footer: 950, 1.2k, 34k. */
export function formatTokens(n: number): string {
	if (n < 1000) return String(n)
	if (n < 10_000) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k`
	return `${Math.round(n / 1000)}k`
}
