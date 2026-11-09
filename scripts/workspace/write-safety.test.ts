import { describe, expect, it } from 'vitest'
import type { WriteGuardPrompt, WriteRowPreview } from '../../src/lib/core/contracts/engines'
import {
	databaseEnvironment,
	databaseMark,
	hasProduction,
	isProductionDatabase,
	markAction,
	productionLabel,
	worstEnvironment,
} from '../../src/lib/features/connections/environments'
import { guardCopy } from '../../src/lib/features/workspace/session/guard-copy'
import {
	isNewColumn,
	previewCell,
	previewIsEmpty,
	previewSummary,
} from '../../src/lib/features/workspace/session/row-preview'
import {
	deadlineFrom,
	effectiveReadOnly,
	formatCountdown,
	TIMED_UNLOCK_CHOICES,
} from '../../src/lib/features/workspace/session/timed-unlock'

type Env = 'local' | 'staging' | 'production'

function profile(environment: Env, marks: [string, string, string?][] = []) {
	return {
		name: 'Dev server',
		environment,
		databaseEnvironments: marks.map(([database, env, schema]) => ({
			database,
			environment: env,
			schema: schema ?? null,
		})),
	}
}

describe('per-database production marks', () => {
	it('a marked database on a local connection is production; others are not', () => {
		const dev = profile('local', [['prod', 'production']])
		expect(isProductionDatabase(dev, 'prod')).toBe(true)
		expect(isProductionDatabase(dev, 'PROD')).toBe(true)
		expect(isProductionDatabase(dev, 'scratch')).toBe(false)
		expect(isProductionDatabase(dev, null)).toBe(false)
		expect(hasProduction(dev)).toBe(true)
		expect(hasProduction(profile('local'))).toBe(false)
		expect(worstEnvironment(dev)).toBe('production')
	})

	it('no marks keeps the connection environment for every database', () => {
		const prod = profile('production')
		expect(isProductionDatabase(prod, 'anything')).toBe(true)
		expect(databaseEnvironment(profile('staging'), 'x')).toBe('staging')
		expect(databaseEnvironment({ environment: 'local' }, 'x')).toBe('local')
	})

	it('a local mark relaxes one database of a production connection', () => {
		const prod = profile('production', [['scratch', 'local']])
		expect(isProductionDatabase(prod, 'scratch')).toBe(false)
		expect(isProductionDatabase(prod, 'app')).toBe(true)
	})

	it('a schema mark beats its database, which beats the connection', () => {
		const dev = profile('local', [
			['app', 'production', 'billing'],
			['app', 'staging'],
		])
		expect(databaseEnvironment(dev, 'app', 'billing')).toBe('production')
		expect(databaseEnvironment(dev, 'app', 'public')).toBe('staging')
		expect(databaseEnvironment(dev, 'app')).toBe('staging')
		expect(databaseMark(dev, 'app', 'billing')?.environment).toBe('production')
		expect(databaseMark(dev, 'app')?.environment).toBe('staging')
	})

	it('the explorer action marks, unmarks or relaxes', () => {
		expect(markAction(profile('local'), 'prod')).toEqual({
			label: 'Mark as production',
			environment: 'production',
		})
		expect(markAction(profile('local', [['prod', 'production']]), 'prod')).toEqual({
			label: 'Remove production mark',
			environment: null,
		})
		expect(markAction(profile('production'), 'scratch')).toEqual({
			label: 'Mark as not production',
			environment: 'local',
		})
		expect(markAction(profile('production', [['scratch', 'local']]), 'scratch')).toEqual({
			label: 'Mark as production',
			environment: null,
		})
	})

	it('the marker tooltip names what is production', () => {
		expect(productionLabel(profile('local', [['prod', 'production']]), 'prod')).toContain(
			'“prod”',
		)
		expect(productionLabel(profile('production'))).toContain('“Dev server”')
	})
})

describe('timed write unlock', () => {
	it('offers one and five minutes', () => {
		expect(TIMED_UNLOCK_CHOICES.map((c) => c.seconds)).toEqual([60, 300])
	})

	it('counts down in minutes and seconds', () => {
		expect(formatCountdown(300_000)).toBe('5:00')
		expect(formatCountdown(61_001)).toBe('1:02')
		expect(formatCountdown(999)).toBe('0:01')
		expect(formatCountdown(-5)).toBe('0:00')
	})

	it('takes the deadline from the backend remaining time', () => {
		expect(deadlineFrom(60_000, 1_000)).toBe(61_000)
		expect(deadlineFrom(null, 1_000)).toBeNull()
		expect(deadlineFrom(0, 1_000)).toBeNull()
	})

	it('lifts read-only only while the window runs', () => {
		expect(effectiveReadOnly(true, null, 10)).toBe(true)
		expect(effectiveReadOnly(true, 20, 10)).toBe(false)
		expect(effectiveReadOnly(true, 20, 20)).toBe(true)
		expect(effectiveReadOnly(false, null, 10)).toBe(false)
	})
})

function preview(extra: Partial<WriteRowPreview>): WriteRowPreview {
	return {
		available: true,
		columns: [],
		rows: [],
		newColumns: [],
		truncated: false,
		...extra,
	}
}

describe('affected-row preview', () => {
	it('says how many rows change and how many are shown', () => {
		expect(previewSummary(preview({ action: 'update', total: 1, rows: [[1]] }))).toBe(
			'1 row would be updated.',
		)
		expect(
			previewSummary(preview({ action: 'delete', total: 120, rows: [[1], [2]] })),
		).toBe('120 rows would be deleted. Showing the first 2.')
		expect(previewSummary(preview({ action: 'insert', total: 2, rows: [[1], [2]] }))).toBe(
			'2 rows would be inserted.',
		)
	})

	it('makes zero rows obvious', () => {
		const none = preview({ action: 'delete', total: 0 })
		expect(previewSummary(none)).toContain('0 rows')
		expect(previewIsEmpty(none)).toBe(true)
	})

	it('explains an unavailable preview, a failed read and a missing count', () => {
		expect(
			previewSummary(preview({ available: false, reason: 'Row preview covers one table.' })),
		).toBe('Preview unavailable. Row preview covers one table.')
		expect(previewSummary(preview({ action: 'update', error: 'permission denied' }))).toBe(
			'Preview failed: permission denied',
		)
		expect(
			previewSummary(
				preview({
					action: 'update',
					rows: [[1]],
					truncated: true,
					note: 'Counting took longer than 3 seconds.',
				}),
			),
		).toBe('1 row or more would be updated. Counting took longer than 3 seconds.')
	})

	it('renders cells as text and marks the new columns', () => {
		expect(previewCell(null)).toBe('NULL')
		expect(previewCell('<b>x</b>')).toBe('<b>x</b>')
		expect(previewCell({ $large: true, preview: 'abc' })).toBe('abc…')
		expect(previewCell({ a: 1 })).toBe('{"a":1}')
		const p = preview({ newColumns: ['name (new)'] })
		expect(isNewColumn(p, 'name (new)')).toBe(true)
		expect(isNewColumn(p, 'name')).toBe(false)
	})
})

describe('the write dialog names a marked database', () => {
	const base: WriteGuardPrompt = {
		code: 'writeGuard',
		kind: 'unlock',
		environment: 'production',
		connectionName: 'Dev server',
		preview: 'UPDATE prod.t SET a = 1',
	}

	it('per-database: the database and the connection', () => {
		expect(guardCopy({ ...base, database: 'prod' }).description).toContain(
			'Database “prod” on “Dev server” is production.',
		)
	})

	it('connection-level: the connection only', () => {
		expect(guardCopy(base).description).toContain('“Dev server” is production.')
	})
})
