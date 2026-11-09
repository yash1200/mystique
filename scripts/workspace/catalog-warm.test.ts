import { beforeEach, describe, expect, test, vi } from 'vitest'
import { mockIpc } from './mock-ipc'

// E-10: Quick Open loads the catalogs of open sessions that have none, so a
// search finds their tables. Bounded, quiet, and it stops when closed.
type FakeSession = { profile: { id: string; engine: string }; engineSessionId: string | null }
const session = { sessions: [] as FakeSession[], activeConnectionId: null as string | null }
const toasts: string[] = []

vi.mock('#features/toast', () => ({
	showToast: (_tone: string, message: string) => toasts.push(message),
}))
vi.mock('#features/connections', () => ({ isConnectCancelled: () => false }))
vi.mock('#features/workspace/session', async () => {
	const { selectedDatabases } = await import(
		'../../src/lib/features/workspace/session/selected-db.svelte'
	)
	const { selectedDatabaseName } = await import(
		'../../src/lib/features/workspace/session/lookup'
	)
	const never = () => {
		throw new Error('not in this test')
	}
	return {
		selectedDatabases,
		selectedDatabaseName,
		workspaceSession: session,
		sessionDatabase: () => null,
		systemDbNames: () => [],
		engineBindsSessionToDatabase: () => false,
		isSessionMissingError: () => false,
		reopenEngineSession: never,
		requireActiveConnection: () => null,
		isWriteCancelled: () => false,
		runWithWriteGuard: never,
		withLiveSession: never,
	}
})

const { warmSessionCatalogs } = await import('../../src/lib/features/workspace/catalog/live')
const { catalogStore } = await import('../../src/lib/features/workspace/catalog/store.svelte')

const open = (id: string, sid: string | null): FakeSession => ({
	profile: { id, engine: 'postgres' },
	engineSessionId: sid,
})

beforeEach(() => {
	toasts.length = 0
	for (const id of ['a', 'b', 'c', 'd']) catalogStore.forget(id)
})

describe('warm session catalogs', () => {
	test('loads only sessions that are live and have no catalog, two at a time', async () => {
		session.sessions = [open('a', 'sa'), open('b', 'sb'), open('c', null), open('d', 'sd')]
		catalogStore.setDatabases('d', [{ name: 'kept', tables: [] }])
		let running = 0
		let peak = 0
		const calls = mockIpc({
			list_engine_catalog: async (args) => {
				running += 1
				peak = Math.max(peak, running)
				await new Promise((resolve) => setTimeout(resolve, 5))
				running -= 1
				const sid = (args.request as { sessionId: string }).sessionId
				return [{ name: 'app', objects: [{ name: `t_${sid}`, kind: 'table' }] }]
			},
		})
		await warmSessionCatalogs(['a', 'b', 'c', 'd'], new AbortController().signal)
		expect(calls.map((call) => (call.args.request as { sessionId: string }).sessionId)).toEqual([
			'sa',
			'sb',
		])
		expect(peak).toBeLessThanOrEqual(2)
		expect(catalogStore.databases('a')[0]?.tables.map((t) => t.name)).toEqual(['t_sa'])
		expect(catalogStore.databases('d')[0]?.name).toBe('kept')
		expect(catalogStore.liveKey.a).toBe('sa')
	})

	test('starts nothing new once aborted, and stays quiet on failure', async () => {
		session.sessions = [open('a', 'sa'), open('b', 'sb'), open('c', 'sc')]
		const stop = new AbortController()
		const calls = mockIpc({
			list_engine_catalog: () => {
				stop.abort()
				throw new Error('server went away')
			},
		})
		await warmSessionCatalogs(['a', 'b', 'c'], stop.signal, 1)
		expect(calls).toHaveLength(1)
		expect(toasts).toEqual([])
		expect(catalogStore.catalogs.a).toBeUndefined()
		expect(catalogStore.loadInFlight.a).toBe(false)
	})
})
