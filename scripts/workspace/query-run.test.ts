import { beforeEach, describe, expect, test, vi } from 'vitest'

// D-07: the run state machine, out of QueryCanvas. Every state lands in the
// tab's run cache: running → ok / error / blocked / idle (cancelled guard).
const caches = new Map<string, any>()
const txns: Record<string, unknown> = {}
const toasts: string[] = []
const openTabs = new Set(['t1', 't2'])
let batchImpl: (body: any) => Promise<any>
let guardCancelled = false
let explainImpl: (req: any) => Promise<any>
const ran: { sessionId: string; pinned: boolean; params: unknown[] }[] = []
const finished: string[] = []

class Fault extends Error {
	kind: string
	statementIndex?: number
	transactionOutcome?: unknown
	constructor(kind: string, message: string, extra: object = {}) {
		super(message)
		this.kind = kind
		Object.assign(this, extra)
	}
}

vi.mock('#core/ipc', () => ({
	errorMessage: (e: unknown) => String(e),
	startTabRun: (tabId: string) => `exec-${tabId}`,
	finishTabRun: (tabId: string) => finished.push(tabId),
	cancelTabRun: async () => {},
	executeEngineBatch: (body: any) => {
		ran.push({ sessionId: body.sessionId, pinned: false, params: body.params })
		return batchImpl(body)
	},
	executePinnedBatch: (body: any) => {
		ran.push({ sessionId: body.sessionId, pinned: true, params: body.params })
		return batchImpl(body)
	},
	toIpcError: (e: any) =>
		e instanceof Fault ? e : { kind: 'internal', message: String(e?.message ?? e) },
	explainEngineQuery: (req: any) => explainImpl(req),
	pinnedTabStatus: async () => ({ pinned: true, inTransaction: false }),
	pinQueryTab: async () => ({ pinned: true, inTransaction: false }),
	unpinQueryTab: async () => ({ pinned: false, inTransaction: false }),
	commitPinnedTab: async () => ({ pinned: true, inTransaction: false }),
	rollbackPinnedTab: async () => ({ pinned: true, inTransaction: false }),
}))
vi.mock('#features/toast', () => ({
	showToast: (_tone: string, message: string) => toasts.push(message),
}))
vi.mock('#features/workspace/session', () => ({
	workspaceSession: {
		getQueryRunCache: (id: string) => caches.get(id),
		setQueryRunCache: (id: string, cache: unknown) => caches.set(id, cache),
		hasOpenTab: (id: string) => openTabs.has(id),
		setTabTxn: (id: string, status: unknown) => (txns[id] = status),
	},
	isWriteCancelled: (e: unknown) => e === 'cancelled',
	isSessionMissingError: (e: unknown) => e === 'gone',
	runWithWriteGuard: async (_sid: string, fn: (c?: string) => Promise<unknown>) => {
		if (guardCancelled) throw 'cancelled'
		return fn(undefined)
	},
}))

const run = await import('../../src/lib/features/workspace/query/run/run.svelte')
const { runExplain } = await import(
	'../../src/lib/features/workspace/query/explain/explain'
)

const okBatch = {
	results: [{ index: 0, result: { columns: [], rows: [], message: 'SELECT 1' } }],
	stoppedAt: null,
}

function target(over: Partial<import('../../src/lib/features/workspace/query/run/run.svelte').RunTarget> = {}) {
	return {
		tabId: 't1',
		engine: 'postgres',
		language: 'sql',
		mode: 'write' as const,
		timeoutMs: 1000,
		sessionId: 's1',
		database: null,
		collection: '',
		manualCommit: false,
		transactions: true,
		doc: () => '',
		...over,
	}
}

beforeEach(() => {
	caches.clear()
	toasts.length = 0
	ran.length = 0
	finished.length = 0
	guardCancelled = false
	openTabs.clear()
	openTabs.add('t1').add('t2')
	batchImpl = async () => okBatch
	explainImpl = async () => ({ nodes: [] })
})

describe('runQuery', () => {
	test('running then ok, with history and the submitted text', async () => {
		let seen: string | undefined
		batchImpl = async () => {
			seen = run.runStateOf('t1').status
			return okBatch
		}
		const history = vi.fn()
		const start = vi.fn()
		const out = await run.runQuery(
			target({ onhistory: history, onstart: start }),
			' select 1 ',
			4,
		)
		expect(out).toEqual({ kind: 'done' })
		expect(seen).toBe('running')
		expect(start).toHaveBeenCalledOnce()
		const cache = caches.get('t1')
		expect(cache.runState).toMatchObject({ status: 'ok', message: 'SELECT 1', mode: 'write' })
		expect(cache).toMatchObject({ submittedText: 'select 1', submittedStart: 4, resultPage: 0 })
		expect(history).toHaveBeenCalledWith(null)
		expect(finished).toEqual(['t1'])
		expect(ran[0]).toMatchObject({ sessionId: 's1', pinned: false, params: [] })
	})

	test('placeholders ask for values first and do not run', async () => {
		const out = await run.runQuery(target(), 'select * from t where id = :id')
		expect(out).toMatchObject({ kind: 'params', text: 'select * from t where id = :id' })
		expect(ran).toEqual([])
		expect(caches.has('t1')).toBe(false)
		await run.runQuery(target(), 'select * from t where id = :id', 0, [7])
		expect(ran[0]!.params).toEqual([7])
	})

	test('an engine error becomes an error state', async () => {
		batchImpl = async () => {
			throw new Fault('query', 'connector error: relation "x" does not exist', {
				statementIndex: 1,
			})
		}
		await run.runQuery(target(), 'select * from x; select 2')
		const cache = caches.get('t1')
		expect(cache.runState.status).toBe('error')
		expect(cache.runState.message).toBe('relation "x" does not exist')
		expect(cache.activeIndex).toBe(1)
	})

	test('a policy refusal is blocked, not an error', async () => {
		batchImpl = async () => {
			throw new Fault('policy', 'Read-only connection')
		}
		await run.runQuery(target(), 'delete from t')
		expect(caches.get('t1').runState).toMatchObject({
			status: 'blocked',
			message: 'Read-only connection',
		})
	})

	test('cancelling the write guard goes back to idle', async () => {
		guardCancelled = true
		await run.runQuery(target(), 'drop table t')
		expect(caches.get('t1').runState).toEqual({ status: 'idle' })
	})

	test('no session and no reconnect is an error', async () => {
		await run.runQuery(target({ sessionId: null }), 'select 1')
		expect(caches.get('t1').runState.status).toBe('error')
		expect(ran).toEqual([])
	})

	test('a read run on a dropped session reconnects and retries once', async () => {
		let calls = 0
		batchImpl = async () => {
			calls += 1
			if (calls === 1) throw 'gone'
			return okBatch
		}
		await run.runQuery(
			target({ mode: 'read', reconnect: async () => 's2' }),
			'select 1',
		)
		expect(ran.map((r) => r.sessionId)).toEqual(['s1', 's2'])
		expect(caches.get('t1').runState.status).toBe('ok')
	})

	test('a write run on a dropped session does not retry', async () => {
		batchImpl = async () => {
			throw 'gone'
		}
		await run.runQuery(target({ reconnect: async () => 's2' }), 'update t set a = 1')
		expect(ran).toHaveLength(1)
		expect(caches.get('t1').runState.status).toBe('error')
	})

	test('a second run while one is running is skipped', async () => {
		caches.set('t1', { runState: { status: 'running' }, resultPage: 0 })
		expect(await run.runQuery(target(), 'select 1')).toEqual({ kind: 'skipped' })
		expect(ran).toEqual([])
	})

	test('the result goes to the tab that started the run', async () => {
		let release: (() => void) | undefined
		batchImpl = () => new Promise((r) => (release = () => r(okBatch)))
		// The canvas reads its props live; the target is a snapshot.
		const t = target({ tabId: 't1' })
		const pending = run.runQuery(t, 'select 1')
		t.tabId = 't2'
		await vi.waitFor(() => expect(release).toBeTypeOf('function'))
		release!()
		await pending
		expect(caches.get('t1').runState.status).toBe('ok')
		expect(caches.has('t2')).toBe(false)
	})

	test('a run whose tab closed writes nothing more', async () => {
		batchImpl = async () => {
			openTabs.delete('t1')
			return okBatch
		}
		const history = vi.fn()
		await run.runQuery(target({ onhistory: history }), 'select 1')
		expect(caches.get('t1').runState.status).toBe('running')
		expect(history).not.toHaveBeenCalled()
	})

	test('a manual-commit run uses the pinned session', async () => {
		await run.runQuery(target({ manualCommit: true }), 'insert into t values (1)')
		expect(ran[0]!.pinned).toBe(true)
	})
})

describe('results and pin state', () => {
	test('select statement shows that statement and resets the page', async () => {
		const batch = {
			results: [
				{ index: 0, result: { columns: [], rows: [], message: 'one' } },
				{ index: 1, result: { columns: [], rows: [], message: 'two' } },
			],
			stoppedAt: null,
		}
		batchImpl = async () => batch
		await run.runQuery(target(), 'select 1; select 2')
		run.setResultPage('t1', 3)
		run.selectStatement('t1', 1)
		const cache = caches.get('t1')
		expect(cache.activeIndex).toBe(1)
		expect(cache.resultPage).toBe(0)
		expect(cache.runState.message).toBe('two')
	})

	test('Run all on Mongo opens on the statement that stopped the batch and marks it', async () => {
		const doc = '\ndb.a.find({})\ndb.boom.find({})\ndb.c.find({})\n'
		const text = doc.trim()
		const second = text.indexOf('db.boom')
		let sent: any
		batchImpl = async (body) => {
			sent = body
			return {
				results: [
					{
						index: 0,
						sqlRange: { start: 0, end: 13 },
						kind: 'read',
						result: { columns: [], rows: [], message: 'ok' },
					},
					{
						index: 1,
						sqlRange: { start: second, end: second + 16 },
						kind: 'read',
						result: null,
						error: 'collection boom is on fire',
						errorPosition: { offset: second, line: 2, column: 1 },
					},
				],
				stoppedAt: 1,
			}
		}
		await run.runQuery(
			target({ engine: 'mongodb', language: 'mql', collection: 'a', doc: () => doc }),
			text,
			1,
		)
		expect(sent.text).toBe(text)
		expect(sent.collection).toBe('a')
		const cache = caches.get('t1')
		expect(cache.runState.status).toBe('ok')
		expect(cache.runState.batch.results).toHaveLength(2)
		expect(cache.activeIndex).toBe(1)
		expect(cache.diagnostic).toMatchObject({
			message: 'collection boom is on fire',
			from: 1 + second,
			line: 3,
			column: 1,
		})
	})

	test('a backend rollback is told once per tab', () => {
		const rolled = { pinned: true, inTransaction: false, rolledBack: true }
		run.applyPinStatus('t1', rolled)
		run.applyPinStatus('t1', rolled)
		run.applyPinStatus('t2', rolled)
		expect(toasts).toHaveLength(2)
		run.applyPinStatus('t1', { pinned: true, inTransaction: false })
		run.applyPinStatus('t1', rolled)
		expect(toasts).toHaveLength(3)
		expect(txns.t1).toEqual(rolled)
	})

	test('clearDiagnostic keeps the run and drops the mark', () => {
		caches.set('t1', {
			runState: { status: 'error', message: 'x', mode: 'read', elapsedMs: 1 },
			resultPage: 0,
			diagnostic: { message: 'x', from: 0, to: 1, line: 1, column: 1 },
		})
		run.clearDiagnostic('t1')
		expect(caches.get('t1').diagnostic).toBeNull()
		expect(caches.get('t1').runState.status).toBe('error')
	})
})

describe('runExplain', () => {
	test('returns the plan and leaves the tab idle', async () => {
		const plan = await runExplain(target(), 'select 1', true)
		expect(plan).toEqual({ nodes: [] })
		expect(caches.get('t1')).toMatchObject({
			runState: { status: 'idle' },
			submittedText: 'select 1',
		})
	})

	test('a failure shows in the run state and returns no plan', async () => {
		explainImpl = async () => {
			throw new Fault('query', 'syntax error')
		}
		expect(await runExplain(target(), 'selec 1', false)).toBeNull()
		expect(caches.get('t1').runState).toMatchObject({
			status: 'error',
			message: 'syntax error',
		})
	})
})
