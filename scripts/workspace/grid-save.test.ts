import { afterEach, describe, expect, it, vi } from 'vitest'
import { parseReviewBeforeSaving } from '../../src/lib/app/prefs-types'
import { latestRun } from '../../src/lib/core/util/latest-run'
import {
	reviewPrompt,
	reviewsEverySave,
	savedMessage,
	undoErrorMessage,
	undoSave,
} from '../../src/lib/features/workspace/table/save-flow'

describe('review before saving', () => {
	it('defaults to production and only "always" adds a dialog', () => {
		expect(parseReviewBeforeSaving(null)).toBe('production')
		expect(parseReviewBeforeSaving('bogus')).toBe('production')
		expect(parseReviewBeforeSaving('always')).toBe('always')
		expect(reviewsEverySave('never')).toBe(false)
		// Production asks once per session through the backend guard, not here.
		expect(reviewsEverySave('production')).toBe(false)
		expect(reviewsEverySave('always')).toBe(true)
	})

	it('the review dialog carries the SQL and the environment', () => {
		const p = reviewPrompt({
			production: true,
			connectionName: 'Prod',
			statements: ['UPDATE t SET n = 1'],
		})
		expect(p.kind).toBe('review')
		expect(p.environment).toBe('production')
		expect(p.statements).toEqual(['UPDATE t SET n = 1'])
		expect(reviewPrompt({ production: false, connectionName: 'L', statements: [] }).environment).toBe(
			'local',
		)
	})
})

describe('saved toast', () => {
	it('counts every saved row', () => {
		expect(savedMessage({ updated: 2, inserted: 1, deleted: 0, statements: [] })).toBe('Saved 3 rows')
		expect(savedMessage({ updated: 1, inserted: 0, deleted: 0, statements: [] })).toBe('Saved 1 row')
		expect(savedMessage({ updated: 0, inserted: 0, deleted: 0, statements: [] })).toBe('Saved')
	})
})

describe('undo', () => {
	const cancelled = (err: unknown) => err instanceof Error && err.message === 'Write cancelled.'

	it('runs the rollback', async () => {
		const rollback = vi.fn(async () => {})
		expect(await undoSave(rollback, cancelled)).toEqual({ ok: true })
		expect(rollback).toHaveBeenCalledTimes(1)
	})

	it('a stale refusal becomes a readable error', async () => {
		const outcome = await undoSave(async () => {
			throw {
				kind: 'rollback_stale',
				message: 'The rows changed after this save. Rollback would overwrite newer data.',
			}
		}, cancelled)
		expect(outcome).toEqual({
			ok: false,
			message: 'Undo refused: the rows changed after this save.',
		})
		expect(undoErrorMessage({ kind: 'policy', message: 'This save was already rolled back' })).toBe(
			'This save was already rolled back',
		)
	})

	it('closing the guard dialog is silent', async () => {
		const outcome = await undoSave(async () => {
			throw new Error('Write cancelled.')
		}, cancelled)
		expect(outcome).toEqual({ ok: false, message: null })
	})
})

describe('latestRun (live SQL panel, structure auto-plan)', () => {
	afterEach(() => {
		vi.useRealTimers()
	})

	it('debounces and keeps only the newest result', async () => {
		vi.useFakeTimers()
		const results: string[] = []
		let n = 0
		const resolvers: Array<(v: string) => void> = []
		const live = latestRun({
			delayMs: 300,
			run: () => {
				n += 1
				return new Promise<string>((resolve) => resolvers.push(resolve))
			},
			onResult: (v) => results.push(v),
			onError: () => {},
		})
		live.schedule()
		live.schedule()
		await vi.advanceTimersByTimeAsync(300)
		expect(n).toBe(1)
		live.schedule()
		await vi.advanceTimersByTimeAsync(300)
		expect(n).toBe(2)
		// The older plan finishes last and is dropped.
		resolvers[1]('new')
		resolvers[0]('old')
		await vi.advanceTimersByTimeAsync(0)
		expect(results).toEqual(['new'])
	})

	it('cancel drops a pending run', async () => {
		vi.useFakeTimers()
		const run = vi.fn(async () => 'x')
		const live = latestRun({ delayMs: 100, run, onResult: () => {}, onError: () => {} })
		live.schedule()
		live.cancel()
		await vi.advanceTimersByTimeAsync(200)
		expect(run).not.toHaveBeenCalled()
	})
})
