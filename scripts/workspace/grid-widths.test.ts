import { describe, expect, it } from 'vitest'
import {
	EMPTY_COLUMN_LAYOUT,
	layoutColumns,
	layoutToStored,
	moveColumn,
	storedToLayout,
	toggleFrozen,
	toggleHidden,
	withColumnWidth,
	type GridColumnLayout,
} from '../../src/lib/features/workspace/grid/column-layout'
import {
	COL_W_AUTO_MAX,
	COL_W_DEFAULT,
	COL_W_FIT_MAX,
	COL_W_MAX,
	COL_W_MIN,
	autoColumnWidths,
	columnOffsets,
	columnWidth,
	dragWidth,
	fitColumnWidth,
	headerWidth,
	sampleRowIndexes,
} from '../../src/lib/features/workspace/grid/column-widths'
import { computeColumnWindow } from '../../src/lib/features/workspace/grid/virtual'

const columns = [
	{ name: 'id', type: 'int4', pk: true },
	{ name: 'flag', type: 'bool' },
	{ name: 'note', type: 'text' },
	{ name: 'a_rather_long_column_name', type: 'timestamp with time zone' },
]

describe('automatic column width', () => {
	it('fits the header when there are no rows', () => {
		const widths = autoColumnWidths(columns, [])
		for (const column of columns) {
			expect(widths[column.name]).toBe(Math.max(COL_W_MIN, headerWidth(column)))
		}
		expect(widths.a_rather_long_column_name).toBeGreaterThan(widths.flag ?? 0)
	})

	it('grows with sampled content and stops at the cap', () => {
		const rows = [
			{ id: 1, flag: true, note: 'x'.repeat(5000), a_rather_long_column_name: null },
			{ id: 2, flag: false, note: 'short', a_rather_long_column_name: null },
		]
		const widths = autoColumnWidths(columns, rows)
		expect(widths.note).toBe(COL_W_AUTO_MAX)
		expect(widths.flag).toBe(Math.max(COL_W_MIN, headerWidth(columns[1]!)))
		const medium = autoColumnWidths(columns, [{ note: 'y'.repeat(20) }])
		expect(medium.note).toBeGreaterThan(Math.max(COL_W_MIN, headerWidth(columns[2]!)))
		expect(medium.note).toBeLessThan(COL_W_AUTO_MAX)
	})

	it('samples the head, the tail, and between', () => {
		expect(sampleRowIndexes(5)).toEqual([0, 1, 2, 3, 4])
		const sample = sampleRowIndexes(10_000)
		expect(sample).toHaveLength(100)
		expect(sample[0]).toBe(0)
		expect(sample.at(-1)).toBe(9_999)
		expect(sample.some((i) => i > 1000 && i < 9000)).toBe(true)
	})

	it('double-click fit uses every row and the wider cap', () => {
		const rows = Array.from({ length: 300 }, (_, i) => ({ note: i === 299 ? 'z'.repeat(60) : 'a' }))
		const fit = fitColumnWidth(columns[2]!, rows)
		expect(fit).toBeGreaterThan(autoColumnWidths(columns, rows.slice(0, 5)).note ?? 0)
		expect(fitColumnWidth(columns[2]!, [{ note: 'q'.repeat(5000) }])).toBe(COL_W_FIT_MAX)
	})
})

describe('resize', () => {
	it('clamps a drag and undoes the UI scale', () => {
		expect(dragWidth(200, 50)).toBe(250)
		expect(dragWidth(200, 50, 1.25)).toBe(240)
		expect(dragWidth(100, -500)).toBe(COL_W_MIN)
		expect(dragWidth(100, 99_999)).toBe(COL_W_MAX)
	})

	it('prefers the user width, then the automatic one', () => {
		expect(columnWidth('note', { note: 400 }, { note: 120 })).toBe(400)
		expect(columnWidth('note', {}, { note: 120 })).toBe(120)
		expect(columnWidth('note', undefined, {})).toBe(COL_W_DEFAULT)
	})
})

describe('widths follow the visible column layout', () => {
	const stored = { id: 70, flag: 90, note: 300, a_rather_long_column_name: 150 }

	function drawn(layout: GridColumnLayout) {
		const placed = layoutColumns(columns, layout)
		const width = (name: string) => columnWidth(name, layout.widths, {})
		return {
			frozen: placed.frozen.map((c) => [c.name, width(c.name)] as const),
			scroll: placed.scroll.map((c) => [c.name, width(c.name)] as const),
		}
	}

	it('keeps each width on its column through hide, reorder and freeze', () => {
		let layout: GridColumnLayout = { ...EMPTY_COLUMN_LAYOUT, widths: stored }
		layout = toggleHidden(layout, 'flag')
		layout = moveColumn(layout, columns.map((c) => c.name), 'a_rather_long_column_name', -1)
		layout = toggleFrozen(layout, 'note')
		const view = drawn(layout)
		expect(view.frozen).toEqual([['note', 300]])
		expect(view.scroll).toEqual([
			['id', 70],
			['a_rather_long_column_name', 150],
		])
	})

	it('puts frozen columns at the sum of the frozen widths before them', () => {
		const indexW = 40
		const layout = toggleFrozen(
			toggleFrozen({ ...EMPTY_COLUMN_LAYOUT, widths: stored }, 'id'),
			'note',
		)
		const frozen = layoutColumns(columns, layout).frozen.map((c) =>
			columnWidth(c.name, layout.widths, {}),
		)
		const offsets = columnOffsets(frozen)
		expect(offsets.map((x) => indexW + x)).toEqual([40, 110, 410])
	})
})

describe('variable-width column window', () => {
	it('matches the uniform window when widths are equal', () => {
		const base = { totalCols: 80, scrollLeft: 0, viewportW: 1100, colW: 128, frozenW: 40, scrollDir: 1 as const }
		const uniform = computeColumnWindow(base)
		const listed = computeColumnWindow({ ...base, widths: Array(80).fill(128) })
		expect(listed.start).toBe(uniform.start)
		expect(Math.abs(listed.end - uniform.end)).toBeLessThanOrEqual(4)
	})

	it('mounts fewer wide columns and starts at the scrolled column', () => {
		const widths = Array.from({ length: 60 }, (_, i) => (i < 10 ? 600 : 80))
		const start = computeColumnWindow({
			totalCols: 60,
			scrollLeft: 0,
			viewportW: 1200,
			colW: 128,
			widths,
			frozenW: 40,
			scrollDir: 1,
		})
		expect(start.start).toBe(0)
		expect(start.end).toBeLessThan(20)
		const later = computeColumnWindow({
			totalCols: 60,
			scrollLeft: 6000,
			viewportW: 1200,
			colW: 128,
			widths,
			frozenW: 40,
			scrollDir: 1,
		})
		expect(later.start).toBeLessThanOrEqual(10)
		expect(later.end).toBeGreaterThan(20)
	})

	it('mounts everything when all columns fit', () => {
		expect(
			computeColumnWindow({
				totalCols: 3,
				scrollLeft: 0,
				viewportW: 1200,
				colW: 128,
				widths: [100, 200, 300],
				frozenW: 40,
				scrollDir: 1,
			}),
		).toEqual({ start: 0, end: 3 })
	})
})

describe('column width persistence', () => {
	it('round-trips through the IPC record', () => {
		let layout = withColumnWidth({ ...EMPTY_COLUMN_LAYOUT, frozen: ['id'] }, 'note', 312.4)
		layout = withColumnWidth(layout, 'id', 64)
		const record = layoutToStored('c1', 'db.public.users', layout)
		expect(record).toEqual({
			connectionId: 'c1',
			tableId: 'db.public.users',
			order: [],
			hidden: [],
			frozen: ['id'],
			widths: { note: 312, id: 64 },
		})
		const back = storedToLayout(JSON.parse(JSON.stringify(record)))
		expect(back).toEqual(layout)
	})

	it('clears a width back to automatic and reads old records', () => {
		const layout = withColumnWidth(withColumnWidth(EMPTY_COLUMN_LAYOUT, 'note', 200), 'note', null)
		expect(layout.widths).toEqual({})
		expect(EMPTY_COLUMN_LAYOUT.widths).toEqual({})
		expect(storedToLayout({ order: ['a'], hidden: [], frozen: [] }).widths).toEqual({})
	})
})
