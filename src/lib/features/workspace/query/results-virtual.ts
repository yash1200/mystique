/**
 * Query-results virtualization.
 * The implementation lives in the shared grid core so the table grid can reuse
 * it. This module re-exports that file; the row window, overscan, hysteresis,
 * and 512-character clip are unchanged.
 */
export {
	CELL_TEXT_MAX,
	COL_MAX_PX,
	COL_MIN_PX,
	HYSTERESIS_VIEWPORTS,
	OVERSCAN_LEAD,
	OVERSCAN_MIN,
	OVERSCAN_TRAIL,
	RESULT_HEADER_H,
	cellDisplay,
	computeColumnWindow,
	computeResultWindow,
	measureColLayout,
} from '#features/workspace/data-grid'

export type {
	ResultColLayout,
	ResultColumn,
	ResultWindow,
} from '#features/workspace/data-grid'
