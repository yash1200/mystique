/**
 * Shared grid virtualization.
 * Row windowing, overscan, hysteresis, and the 512-character clip are the
 * query-results behavior. `computeColumnWindow` is the table grid's horizontal
 * window and does not change those row constants.
 *
 *
 * Window updates are hysteresis-based: keep the current slice until the
 * viewport nears the leading edge. Everyday scrolling then composites
 * already-mounted rows. CSS-grid + transform (not table spacer <tr>s) keeps
 * remounts cheap when the window does move.
 *
 * The buffer is asymmetric: WebKit scrolls the overflow layer on the
 * compositor, so the scroll event (and therefore the remount) can land a few
 * frames after the pixels moved. Rows ahead of the motion are what covers
 * that gap; rows behind it are only there to absorb a direction flip.
 */

import { isLargeCell, largeCellText } from './large-cell'

export const RESULT_HEADER_H = 32
/** Floor of rows kept on each side of the viewport. */
export const OVERSCAN_MIN = 16
/** Buffer ahead of the scroll direction, as a multiple of the visible rows. */
export const OVERSCAN_LEAD = 2.5
/** Buffer behind the scroll direction, as a multiple of the visible rows. */
export const OVERSCAN_TRAIL = 1.25
/**
 * Keep the current window until the leading edge is this close (in
 * viewports). Normal wheel/trackpad motion then scrolls already-painted
 * rows — no remount, no blank frame. Must stay below OVERSCAN_TRAIL so a
 * direction flip does not force an immediate remount.
 */
export const HYSTERESIS_VIEWPORTS = 1

export const COL_MIN_PX = 72
/** Matches the previous `max-w-xs` cap. */
export const COL_MAX_PX = 288
const COL_PAD_PX = 20
const CHAR_PX = 7.25

export type ResultColumn = { name: string; dataType?: string | null }

export type ResultWindow = { start: number; end: number }

export type ResultColLayout = {
	idxW: number
	widths: number[]
	total: number
	template: string
}

/**
 * Longest text a cell ever puts in the DOM.
 *
 * A cell is one line of `white-space: nowrap` text, so the engine has to
 * shape and measure the *whole* run before it can place the ellipsis — the
 * clip is a paint-time operation, not a shortcut out of layout. A results set
 * with a JSON or long-text column therefore pays layout proportional to the
 * bytes in the column, not to the pixels on screen: 6k rows x 20 cols of 20k
 * character values put 6.6M characters in the mounted window and turned a
 * fling into 1.2s of layout.
 *
 * 512 monospace characters at the 12px cell size is ~3700px — wider than any
 * single column can stretch to, so nothing visible is lost. Values the server
 * marked large are already a preview; the cell modal loads the rest by key.
 */

export const CELL_TEXT_MAX = 512

const objectText = new WeakMap<object, string>()

function clampText(text: string): string {
	return text.length > CELL_TEXT_MAX ? `${text.slice(0, CELL_TEXT_MAX)}…` : text
}

export function cellDisplay(v: unknown): string {
	if (v === null || v === undefined) return 'NULL'
	if (isLargeCell(v)) return largeCellText(v)
	if (typeof v === 'object') {
		const hit = objectText.get(v)
		if (hit !== undefined) return hit
		let text: string
		try {
			text = JSON.stringify(v)
		} catch {
			text = String(v)
		}
		// Cache the clamped form: the cache lives as long as the result set,
		// so caching the raw string would retain the payload twice over.
		const shown = clampText(text)
		objectText.set(v, shown)
		return shown
	}
	if (typeof v === 'string') return clampText(v)
	return String(v)
}

function estimateLen(v: unknown): number {
	if (v === null || v === undefined) return 4
	if (typeof v === 'string') return v.length
	if (
		typeof v === 'number' ||
		typeof v === 'boolean' ||
		typeof v === 'bigint'
	) {
		return String(v).length
	}
	return 24
}

/**
 * Visible-row window. Reuses `current` until the viewport nears the leading
 * edge so everyday scrolling is native compositing of already-mounted rows.
 */
export function computeResultWindow(opts: {
	totalRows: number
	scrollTop: number
	viewportH: number
	rowH: number
	headerH: number
	scrollDir: 1 | -1
	current?: ResultWindow | null
}): ResultWindow {
	const n = opts.totalRows
	if (n <= 0) return { start: 0, end: 0 }

	const rowH = Math.max(1, opts.rowH)
	const usable = Math.max(
		opts.viewportH > opts.headerH
			? opts.viewportH - opts.headerH
			: opts.viewportH,
		rowH,
	)
	const first = Math.max(
		0,
		Math.min(n - 1, Math.floor(Math.max(0, opts.scrollTop) / rowH)),
	)
	const visibleCount = Math.ceil(usable / rowH) + 1
	const last = Math.min(n, first + visibleCount)
	const lead = Math.max(OVERSCAN_MIN, Math.ceil(visibleCount * OVERSCAN_LEAD))
	const trail = Math.max(OVERSCAN_MIN, Math.ceil(visibleCount * OVERSCAN_TRAIL))
	const keep = Math.max(
		OVERSCAN_MIN,
		Math.ceil(visibleCount * HYSTERESIS_VIEWPORTS),
	)
	const down = opts.scrollDir > 0

	const cur = opts.current
	if (cur && cur.end > cur.start && cur.start < n) {
		const end = Math.min(cur.end, n)
		const inside = first >= cur.start && last <= end
		// Only the leading edge needs headroom; the trailing edge just has to
		// still cover the viewport. Checking both would defeat the hysteresis,
		// because the trailing buffer is deliberately smaller than `keep`.
		const leadOk = down
			? end >= n || end - last >= keep
			: cur.start === 0 || first - cur.start >= keep
		if (inside && leadOk) return cur
	}

	const before = down ? trail : lead
	const after = down ? lead : trail
	const block = Math.max(8, Math.ceil(visibleCount / 2))
	let start = Math.max(0, first - before)
	start = Math.floor(start / block) * block
	let end = Math.min(n, Math.max(start, last + after))
	end = Math.min(n, Math.ceil(end / block) * block)
	if (end <= start) end = Math.min(n, start + visibleCount)
	return { start, end }
}

/**
 * Fixed column widths from headers + a small row sample.
 * `minmax(w, 1fr)` lets few-column results fill the pane without reflow
 * on every virtual window shift.
 */
export function measureColLayout(
	columns: ResultColumn[],
	rows: unknown[][],
	pageStart: number,
): ResultColLayout {
	const lastNum = pageStart + Math.max(rows.length, 1)
	const idxW = Math.max(44, Math.ceil(String(lastNum).length * 8.5) + 18)
	const n = rows.length
	const sample: number[] = []
	if (n <= 80) {
		for (let i = 0; i < n; i++) sample.push(i)
	} else {
		for (let i = 0; i < 40; i++) sample.push(i)
		for (let i = 1; i <= 20; i++) sample.push(Math.floor((n * i) / 21))
		for (let i = n - 20; i < n; i++) sample.push(i)
	}

	const widths = columns.map((col, ci) => {
		let maxChars =
			col.name.length + (col.dataType ? col.dataType.length + 1 : 0)
		for (const i of sample) {
			const len = estimateLen(rows[i]?.[ci])
			if (len > maxChars) maxChars = len
		}
		return Math.min(
			COL_MAX_PX,
			Math.max(COL_MIN_PX, Math.ceil(maxChars * CHAR_PX) + COL_PAD_PX),
		)
	})
	const total = idxW + widths.reduce((a, b) => a + b, 0)
	const parts = [`${idxW}px`]
	for (const w of widths) parts.push(`minmax(${w}px, 1fr)`)
	return { idxW, widths, total, template: parts.join(' ') }
}

/** Table-grid row gutter. Matches `w-10` (2.5rem). */
export const GRID_INDEX_REM = 2.5
/** Table-grid data column. Matches `min-w-32` (8rem). */
export const GRID_COL_REM = 8

export function gridTrackPx(rootFontPx: number): { indexW: number; colW: number } {
	const root = rootFontPx > 0 ? rootFontPx : 16
	return { indexW: root * GRID_INDEX_REM, colW: root * GRID_COL_REM }
}

/**
 * Column overscan is tighter than the row window. A table page is only 100
 * rows, so the row buffer already covers most of them; the column buffer is
 * what keeps an 80×100 page near 2k mounted cells.
 */
const COL_OVERSCAN_MIN = 2
const COL_OVERSCAN_LEAD = 0.75
const COL_OVERSCAN_TRAIL = 0.35
const COL_HYSTERESIS_VIEWPORTS = 0.45

/**
 * Visible data-column window. Same hysteresis as {@link computeResultWindow}:
 * keep the current slice until the viewport nears the leading edge.
 * `frozenW` is the sticky row gutter, which stays on screen.
 */
export function computeColumnWindow(opts: {
	totalCols: number
	scrollLeft: number
	viewportW: number
	colW: number
	frozenW: number
	scrollDir: 1 | -1
	current?: ResultWindow | null
}): ResultWindow {
	const n = opts.totalCols
	if (n <= 0) return { start: 0, end: 0 }

	const colW = Math.max(1, opts.colW)
	const frozen = Math.max(0, opts.frozenW)
	const usable = Math.max(
		opts.viewportW > frozen ? opts.viewportW - frozen : opts.viewportW,
		colW,
	)
	const first = Math.max(
		0,
		Math.min(n - 1, Math.floor(Math.max(0, opts.scrollLeft) / colW)),
	)
	const visibleCount = Math.ceil(usable / colW) + 1
	const last = Math.min(n, first + visibleCount)
	if (frozen + n * colW <= opts.viewportW) return { start: 0, end: n }

	const lead = Math.max(
		COL_OVERSCAN_MIN,
		Math.ceil(visibleCount * COL_OVERSCAN_LEAD),
	)
	const trail = Math.max(
		COL_OVERSCAN_MIN,
		Math.ceil(visibleCount * COL_OVERSCAN_TRAIL),
	)
	const keep = Math.max(
		COL_OVERSCAN_MIN,
		Math.ceil(visibleCount * COL_HYSTERESIS_VIEWPORTS),
	)
	const forward = opts.scrollDir > 0

	const cur = opts.current
	if (cur && cur.end > cur.start && cur.start < n) {
		const end = Math.min(cur.end, n)
		const inside = first >= cur.start && last <= end
		const leadOk = forward
			? end >= n || end - last >= keep
			: cur.start === 0 || first - cur.start >= keep
		if (inside && leadOk) return cur
	}

	const before = forward ? trail : lead
	const after = forward ? lead : trail
	const block = Math.max(4, Math.ceil(visibleCount / 2))
	let start = Math.max(0, first - before)
	start = Math.floor(start / block) * block
	let end = Math.min(n, Math.max(start, last + after))
	end = Math.min(n, Math.ceil(end / block) * block)
	if (end <= start) end = Math.min(n, start + visibleCount)
	return { start, end }
}
