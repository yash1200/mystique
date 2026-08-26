/**
 * Query-results virtualization helpers.
 *
 * Window updates are hysteresis-based: keep the current slice until the
 * viewport nears an edge. Everyday scrolling then composites already-mounted
 * rows. CSS-grid + transform (not table spacer <tr>s) keeps remounts cheap
 * when the window does move.
 */

export const RESULT_HEADER_H = 32
/** Floor of rows kept above/below the viewport when the window moves. */
export const OVERSCAN_MIN = 24
/** Buffer on each side, as a multiple of the visible row count. */
export const OVERSCAN_VIEWPORTS = 3
/**
 * Keep the current window until the viewport is this close to an edge
 * (in viewports). Normal wheel/trackpad motion then scrolls already-painted
 * rows — no remount, no blank frame.
 */
export const HYSTERESIS_VIEWPORTS = 1.5

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

const objectText = new WeakMap<object, string>()

export function cellDisplay(v: unknown): string {
	if (v === null || v === undefined) return 'NULL'
	if (typeof v === 'object') {
		const hit = objectText.get(v)
		if (hit !== undefined) return hit
		let text: string
		try {
			text = JSON.stringify(v)
		} catch {
			text = String(v)
		}
		objectText.set(v, text)
		return text
	}
	return String(v)
}

function estimateLen(v: unknown): number {
	if (v === null || v === undefined) return 4
	if (typeof v === 'string') return v.length
	if (typeof v === 'number' || typeof v === 'boolean' || typeof v === 'bigint') {
		return String(v).length
	}
	return 24
}

/**
 * Visible-row window. Reuses `current` until the viewport nears an edge
 * so everyday scrolling is native compositing of already-mounted rows.
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
	const overscan = Math.max(
		OVERSCAN_MIN,
		Math.ceil(visibleCount * OVERSCAN_VIEWPORTS),
	)
	const keep = Math.max(
		OVERSCAN_MIN,
		Math.ceil(visibleCount * HYSTERESIS_VIEWPORTS),
	)

	const cur = opts.current
	if (cur && cur.end > cur.start && cur.start < n) {
		const end = Math.min(cur.end, n)
		const inside = first >= cur.start && last <= end
		const atStart = cur.start === 0
		const atEnd = end >= n
		const enoughBefore = atStart || first - cur.start >= keep
		const enoughAfter = atEnd || end - last >= keep
		if (inside && enoughBefore && enoughAfter) return cur
	}

	const before = opts.scrollDir < 0 ? Math.ceil(overscan * 1.2) : overscan
	const after = opts.scrollDir > 0 ? Math.ceil(overscan * 1.2) : overscan
	const block = Math.max(8, visibleCount)
	let start = Math.max(0, first - before)
	start = Math.floor(start / block) * block
	let end = Math.min(n, Math.max(start, first + visibleCount + after))
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
		let maxChars = col.name.length + (col.dataType ? col.dataType.length + 1 : 0)
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
