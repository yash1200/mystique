/** Scroll a table grid cell into view, frozen columns and the # gutter included. */
import type { VisibleColumns } from '#features/workspace/data-grid'

type ColumnBox = { left: number; width: number; pinned: number; frozen: boolean }

/** The grid's scrolling element inside its shell. */
function gridScroller(shell: HTMLElement | null): HTMLElement | null {
	return shell?.querySelector<HTMLElement>('[data-grid-scroll]') ?? null
}

/** Snap fully left so the # gutter shows. */
export function pinToRowGutter(shell: HTMLElement | null) {
	const scroller = gridScroller(shell)
	if (scroller) scroller.scrollLeft = 0
}

export function scrollCellIntoView(input: {
	shell: HTMLElement | null
	visible: VisibleColumns
	rowH: number
	/** Where the table placed a column (its own widths). */
	columnBox: (name: string) => ColumnBox | null | undefined
	row: number
	col: string
}) {
	const { shell, visible, rowH, row, col } = input
	const scroller = gridScroller(shell)
	if (!scroller) return
	const colIdx = visible.names.indexOf(col)
	const headerH =
		scroller.querySelector('[data-grid-head]')?.getBoundingClientRect().height ?? 36
	const top = row * rowH
	const viewTop = scroller.scrollTop
	const viewSpan = Math.max(0, scroller.clientHeight - headerH)
	if (top < viewTop) scroller.scrollTop = top
	else if (top + rowH > viewTop + viewSpan) {
		scroller.scrollTop = Math.max(0, top + rowH - viewSpan)
	}
	// Frozen columns never scroll sideways.
	if (colIdx >= 0 && colIdx < visible.frozen) return
	// First scrolling column: snap fully left so the # gutter is not left
	// off-screen after scrolling in from the right. Geometry, not the cell
	// node — a windowed cell is not mounted until this scroll lands.
	if (colIdx <= visible.frozen) {
		scroller.scrollLeft = 0
		return
	}
	const box = input.columnBox(col)
	if (!box || box.frozen) return
	const left = box.left
	const right = left + box.width
	const viewLeft = scroller.scrollLeft
	const viewRight = viewLeft + scroller.clientWidth
	if (left < viewLeft + box.pinned) scroller.scrollLeft = Math.max(0, left - box.pinned)
	else if (right > viewRight) scroller.scrollLeft = right - scroller.clientWidth
}
