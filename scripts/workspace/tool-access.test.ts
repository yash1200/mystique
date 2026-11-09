import { describe, expect, it } from 'vitest'
import {
	approvalQueue,
	callerLabel,
	needLabel,
	secondsLeft,
} from '../../src/lib/features/approvals'
import {
	INHERIT,
	inheritedLabel,
	isAccessLevel,
	levelOptions,
} from '../../src/lib/features/settings/access-levels'
import { setToolAccessLevel } from '../../src/lib/features/settings/api'
import { mockIpc } from './mock-ipc'

const view = { mcpDefault: 'none', agentDefault: 'read', connections: [] }

describe('access levels', () => {
	it('knows the four levels and nothing else', () => {
		for (const l of ['none', 'read', 'write', 'full']) {
			expect(isAccessLevel(l)).toBe(true)
		}
		expect(isAccessLevel('read-only')).toBe(false)
		expect(isAccessLevel(INHERIT)).toBe(false)
	})

	it('shows what an unset level inherits', () => {
		expect(inheritedLabel('read', 'read')).toBe('Read')
		expect(inheritedLabel('none', 'read')).toBe('MCP None, agent Read')
		const options = levelOptions('Read')
		expect(options[0]).toEqual({ value: INHERIT, label: 'Inherit (Read)' })
		expect(options.map((o) => o.value)).toEqual([
			INHERIT,
			'none',
			'read',
			'write',
			'full',
		])
	})

	it('sends a database level and a cleared level as null', async () => {
		const calls = mockIpc({ set_tool_access_level: () => view })
		await setToolAccessLevel('7', 'app', 'write')
		await setToolAccessLevel('7', null, null)
		expect(calls.map((c) => c.args)).toEqual([
			{ connectionId: '7', database: 'app', level: 'write' },
			{ connectionId: '7', database: null, level: null },
		])
	})
})

describe('approval dock', () => {
	it('counts down to zero and never below', () => {
		const at = Date.parse('2026-11-09T10:00:00.000Z')
		expect(secondsLeft('2026-11-09T10:01:30.000Z', at)).toBe(90)
		expect(secondsLeft('2026-11-09T10:00:00.400Z', at)).toBe(1)
		expect(secondsLeft('2026-11-09T09:59:00.000Z', at)).toBe(0)
		expect(secondsLeft('garbage', at)).toBe(0)
	})

	it('labels the caller and the levels', () => {
		expect(callerLabel('mcp')).toBe('MCP client')
		expect(callerLabel('agent')).toBe('Built-in agent')
		expect(needLabel('full', 'read')).toBe('Needs Full; has Read')
	})

	it('sends one decision per click and re-reads the queue', async () => {
		const req = {
			id: 'r1',
			caller: 'mcp',
			connectionId: '7',
			connectionName: 'prod',
			database: 'app',
			environment: 'production',
			sql: 'DELETE FROM t WHERE id = 1',
			needed: 'full',
			granted: 'read',
			requestedAt: '2026-11-09T10:00:00.000Z',
			expiresAt: '2026-11-09T10:01:30.000Z',
		}
		let queue = [req]
		const calls = mockIpc({
			list_tool_approvals: () => queue,
			decide_tool_approval: () => {
				queue = []
				return null
			},
		})
		await approvalQueue.refresh()
		expect(approvalQueue.pending).toHaveLength(1)
		await Promise.all([
			approvalQueue.decide('r1', true),
			approvalQueue.decide('r1', true),
		])
		const decisions = calls.filter((c) => c.command === 'decide_tool_approval')
		expect(decisions).toEqual([
			{ command: 'decide_tool_approval', args: { id: 'r1', approve: true } },
		])
		expect(approvalQueue.pending).toHaveLength(0)
	})
})
