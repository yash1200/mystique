import { describe, expect, it } from 'vitest'
import {
	CELL_TEXT_MAX,
	HYSTERESIS_VIEWPORTS,
	OVERSCAN_LEAD,
	OVERSCAN_MIN,
	cellDisplay,
	computeColumnWindow,
	computeResultWindow,
} from '../../src/lib/features/workspace/data-grid/virtual'

describe('table grid window', () => {
	it('keeps the query-results row overscan and clip', () => {
		expect(OVERSCAN_MIN).toBe(16)
		expect(OVERSCAN_LEAD).toBe(2.5)
		expect(HYSTERESIS_VIEWPORTS).toBe(1)
		expect(CELL_TEXT_MAX).toBe(512)
		const long = 'x'.repeat(12_000)
		expect(cellDisplay(long).length).toBe(513)
		expect(cellDisplay('note')).toBe('note')
		expect(cellDisplay(null)).toBe('NULL')
	})

	it('reuses the row window until the leading edge runs out of headroom', () => {
		const first = computeResultWindow({
			totalRows: 5000,
			scrollTop: 0,
			viewportH: 640,
			rowH: 28,
			headerH: 32,
			scrollDir: 1,
		})
		const held = computeResultWindow({
			totalRows: 5000,
			scrollTop: 28 * 4,
			viewportH: 640,
			rowH: 28,
			headerH: 32,
			scrollDir: 1,
			current: first,
		})
		expect(held).toEqual(first)
		expect(first.end - first.start).toBeGreaterThan(40)
		expect(first.end).toBeLessThan(5000)
	})

	it('mounts well under 8k cells for an 80 by 100 page', () => {
		const rows = computeResultWindow({
			totalRows: 100,
			scrollTop: 0,
			viewportH: 640,
			rowH: 28,
			headerH: 36,
			scrollDir: 1,
		})
		const cols = computeColumnWindow({
			totalCols: 80,
			scrollLeft: 0,
			viewportW: 1100,
			colW: 128,
			frozenW: 40,
			scrollDir: 1,
		})
		const cells = (rows.end - rows.start) * (cols.end - cols.start)
		expect(cols.end - cols.start).toBeLessThan(80)
		expect(cells).toBeGreaterThan(0)
		expect(cells).toBeLessThanOrEqual(2500)
	})

	it('keeps a column window while the viewport stays inside it', () => {
		const first = computeColumnWindow({
			totalCols: 80,
			scrollLeft: 0,
			viewportW: 1100,
			colW: 128,
			frozenW: 40,
			scrollDir: 1,
		})
		const held = computeColumnWindow({
			totalCols: 80,
			scrollLeft: 128,
			viewportW: 1100,
			colW: 128,
			frozenW: 40,
			scrollDir: 1,
			current: first,
		})
		expect(held).toEqual(first)
	})
})
