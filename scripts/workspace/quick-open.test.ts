import { afterEach, describe, expect, it } from 'vitest'
import { sidebarTarget } from '../../src/lib/app/sidebar-target'
import { SearchGeneration } from '../../src/lib/features/workspace/quick-open/generation'
import {
	collectQuickOpenItems,
	rankQuickOpenItems,
	type QuickOpenItem,
	type QuickOpenSource,
} from '../../src/lib/features/workspace/quick-open/items'
import { MATCH_TIER, matchTier } from '../../src/lib/features/workspace/quick-open/score'
import { keymapHarness, type Harness } from './keymap-harness'

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
	const harnesses: Harness[] = []
	afterEach(() => {
		for (const h of harnesses.splice(0)) h.dispose()
		document.body.innerHTML = ''
	})
	function keys(mac: boolean): Harness {
		const h = keymapHarness({ mac })
		harnesses.push(h)
		return h
	}
	const cmd = { metaKey: true }
	const ctrl = { ctrlKey: true }

	it('opens on cmd K (ctrl K off the Mac) and leaves P alone', () => {
		expect(keys(true).press({ ...cmd, code: 'KeyK' })).toEqual(['app.quick-open'])
		expect(keys(false).press({ ...ctrl, code: 'KeyK' })).toEqual(['app.quick-open'])
		expect(keys(true).press({ ...cmd, code: 'KeyP' })).toEqual([])
		expect(keys(false).press({ ...ctrl, code: 'KeyP' })).toEqual([])
	})

	it('does not treat ctrl as cmd on the Mac', () => {
		const mac = keys(true)
		expect(mac.press({ ...ctrl, code: 'KeyK' })).toEqual([])
		expect(mac.press({ ...ctrl, code: 'KeyL' })).toEqual([])
		expect(mac.press({ ...cmd, ...ctrl, code: 'KeyK' })).toEqual([])
		// The Windows key is not the command key.
		expect(keys(false).press({ ...cmd, code: 'KeyK' })).toEqual([])
	})

	it('leaves cmd L to the editor', () => {
		const h = keys(true)
		const editor = document.createElement('div')
		editor.className = 'cm-editor'
		const content = document.createElement('div')
		content.contentEditable = 'true'
		editor.append(content)
		document.body.append(editor)
		expect(h.press({ ...cmd, code: 'KeyL' }, content)).toEqual([])
		expect(h.press({ ...cmd, code: 'KeyL' })).toEqual(['workspace.focus-filter'])
	})

	it('toggles the sidebar, focuses a tab and cycles connections', () => {
		const h = keys(true)
		expect(h.press({ ...cmd, code: 'KeyB' })).toEqual(['app.toggle-sidebar'])
		expect(h.press({ ...cmd, code: 'Digit1' })).toEqual(['workspace.focus-tab'])
		expect(h.press({ ...cmd, code: 'Digit9' })).toEqual(['workspace.focus-tab'])
		expect(h.press({ ...cmd, shiftKey: true, code: 'BracketRight', key: '}' })).toEqual([
			'workspace.next-connection',
		])
		expect(h.press({ ...cmd, shiftKey: true, code: 'BracketLeft', key: '{' })).toEqual([
			'workspace.prev-connection',
		])
	})

	it('runs a held chord once', () => {
		const h = keys(true)
		expect(h.press({ ...cmd, code: 'KeyK' })).toEqual(['app.quick-open'])
		expect(h.press({ ...cmd, code: 'KeyK', repeat: true })).toEqual([])
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
