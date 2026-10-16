import { describe, expect, it } from 'vitest'
import { sidebarTarget } from '../../src/lib/app/sidebar-target'
import { SearchGeneration } from '../../src/lib/features/workspace/quick-open/generation'
import {
	collectQuickOpenItems,
	rankQuickOpenItems,
	type QuickOpenItem,
	type QuickOpenSource,
} from '../../src/lib/features/workspace/quick-open/items'
import { appChord } from '../../src/lib/features/workspace/quick-open/keys'
import { MATCH_TIER, matchTier } from '../../src/lib/features/workspace/quick-open/score'

describe('quick open scoring', () => {
	it('locks the six tiers in order', () => {
		const tiers = [
			matchTier('ord', 'ord'),
			matchTier('ol', 'order_lines'),
			matchTier('ord', 'order_lines'),
			matchTier('lin', 'order_lines'),
			matchTier('rde', 'order_lines'),
			matchTier('ols', 'order_lines'),
		]
		expect(tiers).toEqual([
			MATCH_TIER.exact,
			MATCH_TIER.initials,
			MATCH_TIER.prefix,
			MATCH_TIER.wordPrefix,
			MATCH_TIER.substring,
			MATCH_TIER.subsequence,
		])
		for (let i = 1; i < tiers.length; i++) {
			expect(tiers[i]).toBeGreaterThan(tiers[i - 1] ?? -1)
		}
		expect(matchTier('ol', 'orderLines')).toBe(MATCH_TIER.initials)
	})

	it('ranks an exact name above a prefix, and a prefix above a substring', () => {
		const ranked = rankQuickOpenItems(
			[
				hit('record', ['record']),
				hit('order_lines', ['order_lines']),
				hit('ord', ['ord']),
			],
			'ord',
			10,
		).map((item) => item.title)
		expect(ranked).toEqual(['ord', 'order_lines', 'record'])
	})

	it('lets the current connection beat a tighter match elsewhere', () => {
		const ranked = rankQuickOpenItems(
			[
				hit('ord', ['ord'], false),
				hit('abocrdzz', ['abocrdzz'], true),
			],
			'ord',
			10,
		).map((item) => item.title)
		expect(matchTier('ord', 'abocrdzz')).toBe(MATCH_TIER.subsequence)
		expect(ranked[0]).toBe('abocrdzz')
	})

	it('scores 5000 names in under 100ms', () => {
		const items = Array.from({ length: 5000 }, (_, index) =>
			hit(index % 17 === 0 ? `order_${index}` : `table_${index}`, [
				index % 17 === 0 ? `order_${index}` : `table_${index}`,
			]),
		)
		const started = performance.now()
		const ranked = rankQuickOpenItems(items, 'ord', 40)
		expect(performance.now() - started).toBeLessThan(100)
		expect(ranked.length).toBeGreaterThan(0)
		expect(ranked.length).toBeLessThanOrEqual(40)
	})
})

describe('quick open sources', () => {
	it('offers metrics only when the capability is on', () => {
		const without = collectQuickOpenItems(source({ supportsMetrics: false }))
		const withMetrics = collectQuickOpenItems(source({ supportsMetrics: true }))
		expect(without.some((item) => item.command === 'metrics')).toBe(false)
		expect(withMetrics.some((item) => item.command === 'metrics')).toBe(true)
	})

	it('marks the active connection’s tables as current', () => {
		const items = collectQuickOpenItems(
			source({
				catalogs: [
					{
						connectionId: 'other',
						name: 'Other',
						colorId: 'slate',
						mark: 'Pg',
						tables: [
							{
								id: 'other.public.orders',
								name: 'orders',
								schema: 'public',
								database: 'app',
								kind: 'table',
							},
						],
					},
					{
						connectionId: 'active',
						name: 'Active',
						colorId: 'teal',
						mark: 'Pg',
						tables: [
							{
								id: 'active.public.orders',
								name: 'orders',
								schema: 'public',
								database: 'app',
								kind: 'table',
							},
						],
					},
				],
			}),
		)
		const current = items.find((item) => item.id === 'catalog:active:active.public.orders')
		const other = items.find((item) => item.id === 'catalog:other:other.public.orders')
		expect(current?.onCurrent).toBe(true)
		expect(other?.onCurrent).toBe(false)
	})
})

describe('quick open chords', () => {
	const base = {
		metaKey: true,
		ctrlKey: false,
		altKey: false,
		shiftKey: false,
		repeat: false,
		code: 'KeyK',
	}

	it('opens on cmd/ctrl K or P', () => {
		expect(appChord(base)?.type).toBe('quick-open')
		expect(appChord({ ...base, metaKey: false, ctrlKey: true, code: 'KeyP' })?.type).toBe(
			'quick-open',
		)
	})

	it('toggles the sidebar and focuses a tab', () => {
		expect(appChord({ ...base, code: 'KeyB' })?.type).toBe('toggle-sidebar')
		expect(appChord({ ...base, code: 'KeyL' })?.type).toBe('focus-explorer-filter')
		expect(appChord({ ...base, code: 'Digit1' })).toEqual({
			type: 'focus-tab',
			index: 0,
		})
		expect(appChord({ ...base, shiftKey: true, code: 'BracketRight' })).toEqual({
			type: 'cycle-connection',
			delta: 1,
		})
	})

	it('ignores repeats and zoom reset', () => {
		expect(appChord({ ...base, repeat: true })).toBeNull()
		expect(appChord({ ...base, code: 'Digit0' })).toBeNull()
	})
})

describe('sidebar target', () => {
	it('uses the session rail only in the workspace', () => {
		expect(sidebarTarget('/workspace')).toBe('session-rail')
		expect(sidebarTarget('/workspace/')).toBe('session-rail')
		expect(sidebarTarget('/')).toBe('home-sidebar')
	})
})

describe('search generation', () => {
	it('drops a result that finished after a newer keystroke', () => {
		const generation = new SearchGeneration()
		const first = generation.next()
		const second = generation.next()
		expect(generation.isCurrent(first)).toBe(false)
		expect(generation.isCurrent(second)).toBe(true)
	})
})

function hit(title: string, terms: string[], onCurrent = false): QuickOpenItem {
	return {
		id: title,
		kind: 'catalog',
		title,
		detail: '',
		terms,
		onCurrent,
		showWhenIdle: false,
		connectionId: onCurrent ? 'active' : 'other',
		colorId: 'teal',
		mark: 'Pg',
	}
}

function source(overrides: Partial<QuickOpenSource> = {}): QuickOpenSource {
	return {
		activeConnectionId: 'active',
		inWorkspace: true,
		supportsMetrics: false,
		profiles: [
			{
				id: 'active',
				name: 'Active',
				engine: 'postgres',
				colorId: 'teal',
				environment: 'local',
				mark: 'Pg',
				engineLabel: 'PostgreSQL',
			},
		],
		catalogs: [],
		saved: [],
		history: [],
		...overrides,
	}
}
