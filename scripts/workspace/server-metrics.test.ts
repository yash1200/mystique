import { flushSync, mount, tick, unmount } from 'svelte'
import { beforeAll, describe, expect, it } from 'vitest'
import type { MetricValue, ServerMetricsSnapshot } from '../../src/lib/core/contracts'
import { metricWarns, sessionStateTone } from '../../src/lib/features/workspace/metrics/presentation'
import ServerMetricsView from '../../src/lib/features/workspace/metrics/ui/ServerMetricsView.svelte'
import { mockIpc } from './mock-ipc'

function metric(patch: Partial<MetricValue>): MetricValue {
	return {
		key: 'm',
		label: 'Metric',
		kind: 'count',
		value: 1,
		display: null,
		unit: null,
		max: null,
		available: true,
		unavailableReason: null,
		...patch,
	}
}

function snapshot(): ServerMetricsSnapshot {
	return {
		engine: 'postgres',
		collectedAt: '2026-11-16T10:00:00Z',
		capabilities: {
			overview: true,
			activeSessions: true,
			terminateSession: true,
			slowQueries: true,
			notes: ['pg_stat_statements is not installed.'],
		},
		overview: {
			connections: metric({ key: 'connections', label: 'Connections', value: 90, max: 100 }),
			cacheHitRatio: metric({ key: 'cacheHitRatio', label: 'Cache hit', kind: 'percent', value: 99.5 }),
			databaseSize: metric({ key: 'databaseSize', label: 'Size', kind: 'bytes', value: 3 * 1024 ** 3 }),
			uptime: metric({ key: 'uptime', label: 'Uptime', kind: 'durationMs', value: 90_000 }),
			activeQueries: metric({
				key: 'activeQueries',
				label: 'Active queries',
				available: false,
				unavailableReason: 'Needs pg_monitor',
			}),
			extras: [metric({ key: 'wal', label: 'WAL level', kind: 'text', display: 'replica' })],
		},
		sessions: [
			{ id: '101', user: 'app', database: 'shop', state: 'active', durationMs: 1200, query: 'select 1', canTerminate: true, extras: [] },
			{ id: '102', user: 'report', database: 'shop', state: 'idle in transaction', durationMs: null, query: null, canTerminate: true, extras: [] },
		],
		slowQueries: [
			{ id: 'q1', query: 'select * from orders', durationMs: 2500, meanMs: 800, calls: 12000, database: 'shop', user: 'app', source: 'historical', extras: [] },
		],
	}
}

// happy-dom rejects `finished` when Svelte cancels a running transition (the
// notes Alert leaving on a tab switch). A browser does the same, and Svelte
// never awaits it, so mark it handled.
beforeAll(() => {
	const animate = Element.prototype.animate
	Element.prototype.animate = function (...args: Parameters<typeof animate>) {
		const animation = animate.apply(this, args)
		animation.finished.catch(() => {})
		return animation
	}
})

async function settle() {
	for (let i = 0; i < 6; i++) {
		await tick()
		await new Promise((r) => setTimeout(r, 0))
	}
	flushSync()
}

function tab(root: HTMLElement, label: string): HTMLButtonElement {
	const found = [...root.querySelectorAll<HTMLButtonElement>('[role="tab"]')].find((b) =>
		b.textContent?.trim().startsWith(label),
	)
	if (!found) throw new Error(`no tab "${label}"`)
	return found
}

function typeInto(input: HTMLInputElement, value: string) {
	input.value = value
	input.dispatchEvent(new Event('input', { bubbles: true }))
}

describe('metrics presentation', () => {
	it('flags an idle-in-transaction session, not every idle one', () => {
		expect(sessionStateTone('idle in transaction')).toBe('warning')
		expect(sessionStateTone('idle')).toBe('neutral')
		expect(sessionStateTone('active')).toBe('info')
		expect(sessionStateTone('Waiting for lock')).toBe('warning')
	})

	it('warns near a gauge limit and on a low cache hit ratio', () => {
		expect(metricWarns(metric({ value: 90, max: 100 }))).toBe(true)
		expect(metricWarns(metric({ value: 50, max: 100 }))).toBe(false)
		expect(metricWarns(metric({ key: 'cacheHitRatio', value: 70 }))).toBe(true)
		expect(metricWarns(metric({ value: 99, max: 100, available: false }))).toBe(false)
	})
})

describe('Server metrics tab', () => {
	it('shows the overview, counts, sections and filters', async () => {
		const calls = mockIpc({ fetch_engine_server_metrics: () => snapshot() })
		const target = document.createElement('div')
		document.body.appendChild(target)
		const component = mount(ServerMetricsView, {
			target,
			props: { engine: 'postgres', engineSessionId: 's1', readOnly: false },
		})
		try {
			await settle()
			expect(calls[0]?.args).toMatchObject({ request: { sessionId: 's1', minDurationMs: 1000 } })
			const text = target.textContent ?? ''
			expect(text).toContain('Server metrics')
			expect(text).toContain('of 100')
			expect(text).toContain('3 GB')
			expect(text).toContain('Needs pg_monitor')
			expect(text).toContain('WAL level')
			expect(text).toContain('pg_stat_statements is not installed.')
			expect(tab(target, 'Connections').textContent).toContain('2')
			expect(tab(target, 'Slow queries').textContent).toContain('1')

			tab(target, 'Connections').click()
			await settle()
			expect(target.textContent).toContain('idle in transaction')
			const filter = target.querySelector<HTMLInputElement>('#metrics-filter')!
			typeInto(filter, 'nobody')
			await settle()
			expect(target.textContent).toContain('No connection matches “nobody”')
			const clear = [...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Clear filter')!
			clear.click()
			await settle()
			expect(target.textContent).toContain('report')

			tab(target, 'Slow queries').click()
			await settle()
			expect(target.textContent).toContain('select * from orders')
			expect(target.textContent).toContain('12,000')

			const live = target.querySelector<HTMLButtonElement>('button[aria-pressed]')!
			expect(live.textContent?.trim()).toBe('Live')
			live.click()
			await settle()
			expect(live.textContent?.trim()).toBe('Paused')
		} finally {
			unmount(component)
			target.remove()
		}
	})

	it('keeps visited panes mounted and marks a manual refresh busy', async () => {
		let release: (() => void) | null = null
		let slow = false
		mockIpc({
			fetch_engine_server_metrics: async () => {
				if (slow) await new Promise<void>((r) => (release = r))
				return snapshot()
			},
		})
		const target = document.createElement('div')
		document.body.appendChild(target)
		const component = mount(ServerMetricsView, {
			target,
			props: { engine: 'postgres', engineSessionId: 's1', readOnly: false },
		})
		try {
			await settle()
			// Unvisited panes are not built yet.
			expect(target.querySelector('#metrics-pane-sessions')).toBeNull()
			tab(target, 'Connections').click()
			await settle()
			const sessions = target.querySelector<HTMLElement>('#metrics-pane-sessions')!
			expect(sessions.hidden).toBe(false)
			tab(target, 'Overview').click()
			await settle()
			// Same node, only hidden: switching back rebuilds nothing.
			expect(target.querySelector('#metrics-pane-sessions')).toBe(sessions)
			expect(sessions.hidden).toBe(true)
			tab(target, 'Connections').click()
			await settle()
			expect(target.querySelector('#metrics-pane-sessions')).toBe(sessions)
			expect(sessions.hidden).toBe(false)

			slow = true
			target.querySelector<HTMLButtonElement>('button[aria-label="Refresh metrics"]')!.click()
			await settle()
			expect(sessions.getAttribute('aria-busy')).toBe('true')
			release!()
			await settle()
			expect(sessions.getAttribute('aria-busy')).toBe('false')
		} finally {
			unmount(component)
			target.remove()
		}
	})

	it('shows a failed first load with a retry', async () => {
		let fail = true
		mockIpc({
			fetch_engine_server_metrics: () => {
				if (fail) throw { kind: 'connector', message: 'Permission denied for pg_stat_activity.' }
				return snapshot()
			},
		})
		const target = document.createElement('div')
		document.body.appendChild(target)
		const component = mount(ServerMetricsView, {
			target,
			props: { engine: 'postgres', engineSessionId: 's1', readOnly: false },
		})
		try {
			await settle()
			expect(target.textContent).toContain('Could not load metrics')
			expect(target.textContent).toContain('Permission denied for pg_stat_activity.')
			fail = false
			const retry = [...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Retry')!
			retry.click()
			await settle()
			expect(target.textContent).not.toContain('Could not load metrics')
			expect(target.textContent).toContain('Cache hit')
		} finally {
			unmount(component)
			target.remove()
		}
	})
})
