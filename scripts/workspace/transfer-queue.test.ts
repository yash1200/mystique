import assert from 'node:assert/strict'
import { test } from 'vitest'

import {
	TransferQueue,
	type TransferRun,
} from '../../src/lib/features/workspace/transfer/queue.svelte.ts'

function deferred() {
	let resolve!: (rows: number) => void
	let reject!: (err: Error) => void
	const promise = new Promise<number>((res, rej) => {
		resolve = res
		reject = rej
	})
	return { promise, resolve, reject }
}

function queue() {
	let n = 0
	const cancelled: string[] = []
	const q = new TransferQueue({
		cancel: async (id) => {
			cancelled.push(id)
		},
		newId: () => `id-${++n}`,
	})
	return { q, cancelled }
}

const tick = () => new Promise((r) => setTimeout(r, 0))

test('one transfer at a time per connection; other connections run alongside', async () => {
	const { q } = queue()
	const started: string[] = []
	const gates = { a1: deferred(), a2: deferred(), b1: deferred() }
	const run =
		(name: keyof typeof gates): TransferRun =>
		async (_exec, onRows) => {
			started.push(name)
			onRows(10)
			return { rows: await gates[name].promise }
		}
	const a1 = q.enqueue({ kind: 'export', connectionId: 'a', label: 'a1' }, run('a1'))
	const a2 = q.enqueue({ kind: 'export', connectionId: 'a', label: 'a2' }, run('a2'))
	q.enqueue({ kind: 'import', connectionId: 'b', label: 'b1' }, run('b1'))
	await tick()
	assert.deepEqual(started, ['a1', 'b1'])
	assert.equal(q.active.length, 3)
	assert.equal(q.tasks.find((t) => t.id === a1.id)?.rows, 10)
	assert.equal(q.tasks.find((t) => t.id === a2.id)?.status, 'queued')

	gates.a1.resolve(500)
	const done = await a1.done
	assert.equal(done.status, 'done')
	assert.equal(done.rows, 500)
	await tick()
	assert.deepEqual(started, ['a1', 'b1', 'a2'], 'the next one on a starts')
	gates.a2.reject(new Error('disk full'))
	assert.equal((await a2.done).status, 'failed')
	assert.equal((await a2.done).message, 'disk full')
	gates.b1.resolve(1)
	await tick()
	assert.equal(q.hasActive, false)
	q.clearFinished()
	assert.equal(q.tasks.length, 0)
})

test('cancel drops a queued transfer and asks the backend to stop a running one', async () => {
	const { q, cancelled } = queue()
	const gate = deferred()
	let secondRan = false
	const first = q.enqueue({ kind: 'export', connectionId: 'a', label: 'one' }, async () => ({
		rows: await gate.promise,
	}))
	const second = q.enqueue({ kind: 'export', connectionId: 'a', label: 'two' }, async () => {
		secondRan = true
		return { rows: 0 }
	})
	await q.cancel(second.id)
	assert.equal((await second.done).status, 'cancelled')

	await q.cancel(first.id)
	assert.equal(cancelled.length, 1, 'the running export got a cancel')
	gate.reject(new Error('Query cancelled.'))
	assert.equal((await first.done).status, 'cancelled')
	await tick()
	assert.equal(secondRan, false)
	// Finished entries stay until dismissed.
	assert.equal(q.tasks.length, 2)
	q.dismiss(first.id)
	assert.deepEqual(
		q.tasks.map((t) => t.label),
		['two'],
	)
})
