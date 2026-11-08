import { describe, expect, it, vi } from 'vitest'
import type { WriteGuardPrompt } from '../../src/lib/core/contracts/engines'
import { guardCopy, typedNameOk } from '../../src/lib/features/workspace/session/guard-copy'
import {
	guardLoop,
	isWriteCancelled,
	parseWriteGuard,
	type WriteGuardDecision,
} from '../../src/lib/features/workspace/session/guard-loop'

function prompt(kind: string, extra: Partial<WriteGuardPrompt> = {}): WriteGuardPrompt {
	return {
		code: 'writeGuard',
		kind,
		environment: 'production',
		connectionName: 'Prod',
		preview: 'Save 1 update(s) on t',
		...extra,
	}
}

/** The backend refuses until it gets what the prompt asked for. */
function backend(first: WriteGuardPrompt) {
	const calls: (string | undefined)[] = []
	let unlocked = false
	const run = async (confirm?: string) => {
		calls.push(confirm)
		if (first.kind === 'unlock' && !unlocked) throw new Error(JSON.stringify(first))
		if (first.kind !== 'unlock' && confirm !== first.objectName) {
			throw new Error(JSON.stringify(first))
		}
		return 'ran'
	}
	return { calls, run, unlock: async () => void (unlocked = true) }
}

describe('guardLoop', () => {
	it('unlock: one dialog, then the retry runs', async () => {
		const be = backend(prompt('unlock', { statements: ['UPDATE t SET n = 1'] }))
		const ask = vi.fn(async (): Promise<WriteGuardDecision> => ({ ok: true }))
		await expect(guardLoop(be.run, { ask, unlock: be.unlock })).resolves.toBe('ran')
		expect(ask).toHaveBeenCalledTimes(1)
		expect(ask.mock.calls[0][0].statements).toEqual(['UPDATE t SET n = 1'])
	})

	it('confirm: sends the object name back without unlocking', async () => {
		const be = backend(prompt('confirm', { environment: 'staging', objectName: 't' }))
		const unlock = vi.fn(be.unlock)
		const ask = vi.fn(async (): Promise<WriteGuardDecision> => ({ ok: true }))
		await guardLoop(be.run, { ask, unlock })
		expect(unlock).not.toHaveBeenCalled()
		expect(be.calls).toEqual([undefined, 't'])
	})

	it('typeConfirm: sends what was typed', async () => {
		const be = backend(prompt('typeConfirm', { objectName: 'orders' }))
		const ask = async (): Promise<WriteGuardDecision> => ({ ok: true, typedName: 'orders' })
		await guardLoop(be.run, { ask, unlock: be.unlock })
		expect(be.calls).toEqual([undefined, 'orders'])
	})

	it('a caller dialog can pre-confirm, so the backend asks nothing', async () => {
		const be = backend(prompt('confirm', { objectName: 't' }))
		const ask = vi.fn(async (): Promise<WriteGuardDecision> => ({ ok: true }))
		await guardLoop(be.run, { ask, unlock: be.unlock }, { confirmObjectName: 't' })
		expect(ask).not.toHaveBeenCalled()
	})

	it('cancel is a silent WriteCancelled', async () => {
		const be = backend(prompt('unlock'))
		const err = await guardLoop(be.run, {
			ask: async () => ({ ok: false }),
			unlock: be.unlock,
		}).catch((e) => e)
		expect(isWriteCancelled(err)).toBe(true)
		// Survives the grid API re-wrapping the error.
		expect(isWriteCancelled(new Error((err as Error).message))).toBe(true)
		expect(isWriteCancelled(new Error('duplicate key'))).toBe(false)
	})

	it('passes the caller label to the dialog', async () => {
		const be = backend(prompt('unlock'))
		const ask = vi.fn(async (): Promise<WriteGuardDecision> => ({ ok: true }))
		await guardLoop(be.run, { ask, unlock: be.unlock }, { confirmLabel: 'Save' })
		expect(ask.mock.calls[0][0].confirmLabel).toBe('Save')
	})

	it('other errors pass through untouched', async () => {
		const run = async () => {
			throw new Error('connection lost')
		}
		await expect(
			guardLoop(run, { ask: async () => ({ ok: true }), unlock: async () => {} }),
		).rejects.toThrow('connection lost')
	})
})

describe('parseWriteGuard', () => {
	it('reads the backend JSON, including statements', () => {
		const raw = `write_guard: ${JSON.stringify(prompt('unlock', { statements: ['DELETE FROM t WHERE id = 1'] }))}`
		expect(parseWriteGuard(raw)?.statements).toEqual(['DELETE FROM t WHERE id = 1'])
		expect(parseWriteGuard('policy: read-only')).toBeNull()
	})
})

describe('guard dialog copy', () => {
	it('production unlock shows the SQL and says it lasts the session', () => {
		const copy = guardCopy(
			prompt('unlock', { statements: ['UPDATE t SET n = 1', 'DELETE FROM t WHERE id = 2'] }),
		)
		expect(copy.title).toBe('Allow writes on Production')
		expect(copy.body).toBe('UPDATE t SET n = 1;\nDELETE FROM t WHERE id = 2')
		expect(copy.description).toMatch(/until you disconnect/)
		expect(copy.typed).toBe(false)
	})

	it('type-to-confirm needs the exact name and implies the unlock', () => {
		const p = prompt('typeConfirm', { objectName: 'orders', statements: ['DROP TABLE orders'] })
		const copy = guardCopy(p)
		expect(copy.typed).toBe(true)
		expect(copy.description).toMatch(/until you disconnect/)
		expect(typedNameOk(p, ' ORDERS ')).toBe(true)
		expect(typedNameOk(p, 'order')).toBe(false)
	})

	it('falls back to the preview when there is no SQL', () => {
		expect(guardCopy(prompt('unlock', { preview: 'Rename a to b' })).body).toBe('Rename a to b')
	})
})
