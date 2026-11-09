/** Quick Open: results from every open session, grouped by connection, and a focus trap. */
import { afterEach, describe, expect, it } from 'vitest'
import { focusTrap, nextTabStop, tabbables } from '../../src/lib/core/ui/focus-trap'
import {
	collectQuickOpenItems,
	groupQuickOpenItems,
	rankQuickOpenItems,
	type QuickOpenCatalog,
	type QuickOpenSource,
} from '../../src/lib/features/workspace/quick-open/items'

function catalog(connectionId: string, name: string, tables: string[]): QuickOpenCatalog {
	return {
		connectionId,
		name,
		colorId: connectionId === 'active' ? 'teal' : 'rose',
		mark: 'Pg',
		tables: tables.map((table) => ({
			id: `app.public.${table}`,
			name: table,
			schema: 'public',
			database: 'app',
			kind: 'table',
		})),
	}
}

function source(catalogs: QuickOpenCatalog[]): QuickOpenSource {
	return {
		activeConnectionId: 'active',
		inWorkspace: true,
		supportsMetrics: false,
		profiles: [],
		catalogs,
		saved: [],
		history: [],
	}
}

describe('quick open groups', () => {
	const items = collectQuickOpenItems(
		source([
			catalog('active', 'Local', ['orders', 'users']),
			catalog('other', 'Staging', ['orders', 'order_lines', 'audit']),
		]),
	)

	it('finds tables in every open session, under its connection, current first', () => {
		const groups = groupQuickOpenItems(rankQuickOpenItems(items, 'ord', items.length))
		expect(groups.map((group) => group.label)).toEqual(['Local', 'Staging'])
		expect(groups[0]?.items.map((item) => item.title)).toEqual(['orders'])
		expect(groups[1]?.items.map((item) => item.title)).toEqual(['order_lines', 'orders'])
		expect(groups[1]?.colorId).toBe('rose')
	})

	it('caps each connection so one large catalog cannot fill the list', () => {
		const many = collectQuickOpenItems(
			source([
				catalog('active', 'Local', ['t_a']),
				catalog(
					'other',
					'Big',
					Array.from({ length: 50 }, (_, i) => `t_${i}`),
				),
			]),
		)
		const groups = groupQuickOpenItems(rankQuickOpenItems(many, 't_', many.length), 10, 60)
		expect(groups.map((group) => [group.label, group.items.length])).toEqual([
			['Local', 1],
			['Big', 10],
		])
		const capped = groupQuickOpenItems(rankQuickOpenItems(many, 't_', many.length), 10, 5)
		expect(capped.flatMap((group) => group.items)).toHaveLength(5)
	})

	it('puts commands and connections under their own headings', () => {
		const withCommands = collectQuickOpenItems({
			...source([]),
			profiles: [
				{
					id: 'active',
					name: 'Local',
					engine: 'postgres',
					colorId: 'teal',
					environment: 'local',
					mark: 'Pg',
					engineLabel: 'PostgreSQL',
				},
			],
		})
		const groups = groupQuickOpenItems(rankQuickOpenItems(withCommands, '', 50))
		expect(groups.map((group) => group.label)).toEqual(['Commands', 'Connections'])
	})
})

describe('focus trap', () => {
	afterEach(() => {
		document.body.innerHTML = ''
	})

	function dialog() {
		const outside = document.createElement('button')
		outside.textContent = 'behind'
		const root = document.createElement('div')
		root.setAttribute('role', 'dialog')
		const input = document.createElement('input')
		const option = document.createElement('button')
		option.tabIndex = -1
		const close = document.createElement('button')
		root.append(input, option, close)
		document.body.append(outside, root)
		return { outside, root, input, option, close }
	}

	function tab(shiftKey = false) {
		const event = new KeyboardEvent('keydown', {
			key: 'Tab',
			shiftKey,
			bubbles: true,
			cancelable: true,
		})
		;(document.activeElement ?? document.body).dispatchEvent(event)
		return event
	}

	it('wraps Tab and Shift+Tab and skips tabindex=-1', () => {
		const { root, input, close } = dialog()
		expect(tabbables(root)).toEqual([input, close])
		const trap = focusTrap(root)
		input.focus()
		expect(tab().defaultPrevented).toBe(true)
		expect(document.activeElement).toBe(close)
		tab()
		expect(document.activeElement).toBe(input)
		tab(true)
		expect(document.activeElement).toBe(close)
		trap.destroy()
	})

	it('pulls focus back in while active, and lets go once closed', () => {
		const { outside, root, input } = dialog()
		let open = true
		const trap = focusTrap(root, () => open)
		outside.focus()
		expect(document.activeElement).toBe(input)
		open = false
		outside.focus()
		expect(document.activeElement).toBe(outside)
		trap.destroy()
	})

	it('picks the next stop, entering from outside at either end', () => {
		const a = document.createElement('button')
		const b = document.createElement('button')
		expect(nextTabStop([a, b], null, false)).toBe(a)
		expect(nextTabStop([a, b], null, true)).toBe(b)
		expect(nextTabStop([a, b], b, false)).toBe(a)
		expect(nextTabStop([], null, false)).toBeNull()
	})
})
