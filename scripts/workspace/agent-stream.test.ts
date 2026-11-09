import type { Channel } from '@tauri-apps/api/core'
import { describe, expect, it } from 'vitest'
import type { AgentEvent, AgentMessage } from '../../src/lib/core/contracts'
import { agentChat } from '../../src/lib/features/workspace/agent/chat.svelte'
import { chatAfterConnectionChange } from '../../src/lib/features/workspace/agent/connection-chat'
import { sendAgentMessage } from '../../src/lib/features/workspace/agent/api'
import {
	applyAgentEvent,
	formatTokens,
	newLiveRun,
	type LiveRun,
} from '../../src/lib/features/workspace/agent/live-run'
import {
	activeMention,
	appendMention,
	insertMention,
	mentionCandidates,
	resolveMentions,
} from '../../src/lib/features/workspace/agent/mentions'
import { fixRequestFor } from '../../src/lib/features/workspace/query/run/fix-request'
import { mockIpc } from './mock-ipc'

function run(events: AgentEvent[]): LiveRun {
	return events.reduce(applyAgentEvent, newLiveRun('k', '1'))
}

const tokens = { input: 1200, output: 340, cachedInput: 0 }

function row(id: string, role: string, content: string): AgentMessage {
	return {
		id,
		chatId: '7',
		role,
		content,
		toolName: role === 'tool' ? 'describe_object' : null,
		draftBody: null,
		createdAt: '',
	}
}

describe('live run reducer', () => {
	it('joins text deltas and keeps tool calls in order', () => {
		const r = run([
			{ type: 'textDelta', text: 'Let me ' },
			{ type: 'textDelta', text: 'look.' },
			{ type: 'toolCallStart', id: 'a', name: 'describe_object', args: { name: 'orders' } },
			{ type: 'toolCallEnd', id: 'a', ok: true, summary: '4 columns', messageId: '9' },
			{ type: 'toolCallStart', id: 'b', name: 'sample_rows', args: {} },
			{ type: 'toolCallEnd', id: 'b', ok: false, summary: 'denied' },
			{ type: 'textDelta', text: '\n\nDone.' },
			{ type: 'done', messageId: '10', tokens },
		])
		expect(r.parts.map((p) => p.kind)).toEqual(['text', 'tool', 'tool', 'text'])
		expect(r.parts[0]).toEqual({ kind: 'text', text: 'Let me look.' })
		expect(r.parts[3]).toEqual({ kind: 'text', text: 'Done.' })
		const [, a, b] = r.parts
		expect(a?.kind === 'tool' && a.tool.status).toBe('ok')
		expect(a?.kind === 'tool' && a.tool.messageId).toBe('9')
		expect(b?.kind === 'tool' && b.tool.status).toBe('error')
		expect(r.done?.messageId).toBe('10')
	})

	it('marks a call waiting on the approval dock, then clears it', () => {
		let r = run([
			{ type: 'toolCallStart', id: 'a', name: 'execute_query', args: { query: 'delete from t' } },
			{ type: 'toolApprovalPending', id: 'a', approvalId: 'ap', sql: 'delete from t', needed: 'full' },
		])
		const tool = () => (r.parts[0]?.kind === 'tool' ? r.parts[0].tool : null)
		expect(tool()?.status).toBe('waiting')
		expect(tool()?.approvalSql).toBe('delete from t')
		r = applyAgentEvent(r, { type: 'toolCallEnd', id: 'a', ok: true, summary: '1 rows' })
		expect(tool()?.status).toBe('ok')
		expect(tool()?.approvalSql).toBeUndefined()
	})

	it('records exhausted, compaction and errors; ignores events after the end', () => {
		const r = run([
			{ type: 'contextCompacted', droppedMessages: 4, trimmedResults: 1 },
			{ type: 'exhausted', turns: 16 },
			{ type: 'error', kind: 'timeout', message: 'too slow' },
			{ type: 'textDelta', text: 'late' },
		])
		expect(r.compacted).toBe(4)
		expect(r.exhausted).toBe(16)
		expect(r.error).toEqual({ kind: 'timeout', message: 'too slow' })
		expect(r.parts).toEqual([])
	})

	it('formats token counts', () => {
		expect(formatTokens(950)).toBe('950')
		expect(formatTokens(1200)).toBe('1.2k')
		expect(formatTokens(34_400)).toBe('34k')
	})
})

describe('@table mentions', () => {
	const tables = [
		{ database: 'app', schema: 'public', name: 'orders' },
		{ database: 'app', schema: 'public', name: 'order_items' },
		{ database: 'app', schema: 'audit', name: 'events' },
	]

	it('finds the @query at the caret', () => {
		expect(activeMention('show @ord', 9)).toEqual({ start: 5, query: 'ord' })
		expect(activeMention('@', 1)).toEqual({ start: 0, query: '' })
		expect(activeMention('mail a@b', 8)).toBeNull()
		expect(activeMention('no mention', 10)).toBeNull()
	})

	it('suggests prefix matches first', () => {
		expect(mentionCandidates(tables, 'ord').map((t) => t.name)).toEqual([
			'order_items',
			'orders',
		])
		expect(mentionCandidates(tables, 'audit.').map((t) => t.name)).toEqual(['events'])
	})

	it('inserts and appends references', () => {
		expect(insertMention('show @ord rows', 5, 9, tables[0]!)).toEqual({
			text: 'show @public.orders rows',
			caret: 20,
		})
		expect(appendMention('join', tables[2]!)).toBe('join @audit.events ')
		expect(appendMention('', tables[2]!)).toBe('@audit.events ')
	})

	it('turns references in the text into tables, once each', () => {
		expect(
			resolveMentions('join @public.orders with @events and @orders. @nope', tables),
		).toEqual([
			{ schema: 'public', name: 'orders' },
			{ schema: 'audit', name: 'events' },
		])
	})
})

describe('Fix with AI', () => {
	it('points into the statement that ran, not the editor', () => {
		const editor = 'select 1;\nselect id,\n  nme from users'
		const start = editor.indexOf('select id')
		const submitted = editor.slice(start)
		const from = editor.indexOf('nme')
		expect(fixRequestFor(submitted, start, ' column "nme" does not exist ', { from })).toEqual({
			sql: submitted,
			error: 'column "nme" does not exist',
			line: 2,
			column: 3,
		})
		expect(fixRequestFor('select', 0, 'boom', null)).toEqual({ sql: 'select', error: 'boom' })
	})
})

describe('streaming send', () => {
	it('sends a Channel and resolves before the reply finishes', async () => {
		let channel: Channel<AgentEvent> | null = null
		mockIpc({
			send_agent_message: (args) => {
				channel = args.onEvent as Channel<AgentEvent>
				return { chat: { id: 7, connectionId: 1 }, user: row('1', 'user', 'hi') }
			},
		})
		const seen: AgentEvent[] = []
		const started = await sendAgentMessage(
			{
				connectionId: '1',
				connectionName: 'local',
				engine: 'postgres',
				provider: 'openai',
				model: 'gpt',
				text: 'hi',
			},
			(e) => seen.push(e),
		)
		expect(started.chat.id).toBe('7')
		expect(channel).not.toBeNull()
		channel!.onmessage({ type: 'textDelta', text: 'he' })
		expect(seen).toEqual([{ type: 'textDelta', text: 'he' }])
	})

	it('a two-tool turn ends on the saved final reply, with its tool cards', async () => {
		const saved = [
			row('1', 'user', 'q'),
			row('2', 'tool', '{"args":{},"result":{}}'),
			row('3', 'tool', '{"args":{},"result":{}}'),
			row('4', 'assistant', 'final answer'),
		]
		mockIpc({ list_agent_messages: () => saved })
		agentChat.resetForConnection('1')
		const key = agentChat.beginRun('1')
		agentChat.runStarted(key, '7')
		agentChat.chatId = '7'
		const events: AgentEvent[] = [
			{ type: 'toolCallStart', id: 'a', name: 'describe_object', args: {} },
			{ type: 'toolCallEnd', id: 'a', ok: true, summary: '1 columns' },
			{ type: 'textDelta', text: 'checking more' },
			{ type: 'toolCallStart', id: 'b', name: 'describe_object', args: {} },
			{ type: 'toolCallEnd', id: 'b', ok: true, summary: '2 columns' },
			{ type: 'textDelta', text: 'final answer' },
		]
		for (const e of events) agentChat.onRunEvent(key, e)
		// The first assistant text is not the end: the run is still live.
		expect(agentChat.live).not.toBeNull()
		agentChat.onRunEvent(key, { type: 'done', messageId: '4', tokens })
		await waitFor(() => agentChat.live === null)
		expect(agentChat.messages.map((m) => m.role)).toEqual([
			'user',
			'tool',
			'tool',
			'assistant',
		])
		expect(agentChat.messages.at(-1)?.content).toBe('final answer')
		expect(agentChat.lastTokens).toEqual(tokens)
	})

	it('switching connection drops the old chat id and ignores its stream', () => {
		agentChat.resetForConnection('1')
		agentChat.chatId = '7'
		const key = agentChat.beginRun('1')
		agentChat.resetForConnection('2')
		expect(agentChat.chatId).toBeNull()
		expect(agentChat.live).toBeNull()
		agentChat.onRunEvent(key, { type: 'textDelta', text: 'old connection' })
		expect(agentChat.live).toBeNull()
		expect(
			chatAfterConnectionChange(
				{ connectionId: '1', chatId: '7', chats: [], messages: [] },
				'2',
			).chatId,
		).toBeNull()
	})

	it('Stop before the chat id is known cancels once it arrives', async () => {
		const calls = mockIpc({ cancel_agent_message: () => true })
		agentChat.resetForConnection('3')
		const key = agentChat.beginRun('3')
		agentChat.stopRun()
		expect(agentChat.live?.stopRequested).toBe(true)
		agentChat.runStarted(key, '11')
		await waitFor(() => calls.length > 0)
		expect(calls[0]).toEqual({ command: 'cancel_agent_message', args: { chatId: '11' } })
	})
})

async function waitFor(check: () => boolean): Promise<void> {
	for (let i = 0; i < 100; i += 1) {
		if (check()) return
		await new Promise((r) => setTimeout(r, 5))
	}
	throw new Error('condition never held')
}
