import type { DataFamily } from '#core/contracts'

export type { DataFamily }

export type ErNodeKind = 'table' | 'collection' | 'key_pattern' | 'entity'

/** Normal = solid FK-style; broken = dashed / optional link. */
export type ErEdgeKind =
	| 'normal'
	| 'broken'
	| 'fk'
	| 'inferred'
	| 'logical'
	| 'key_ref'

export interface ErColumn {
	id: string
	name: string
	dataType: string
	pk?: boolean
	nullable?: boolean
}

export interface ErNode {
	id: string
	kind: ErNodeKind
	name: string
	columns: ErColumn[]
	x: number
	y: number
}

export interface ErPoint {
	x: number
	y: number
}

export interface ErEdge {
	id: string
	from: string
	to: string
	fromColumnId: string
	toColumnId: string
	kind: ErEdgeKind
	label?: string
	waypoints?: ErPoint[]
}

export interface ErDiagramState {
	family: DataFamily
	nodes: ErNode[]
	edges: ErEdge[]
}

export type ErFocusedLink = {
	edge: ErEdge
	tableIds: Set<string>
	colIds: Set<string>
}

export type ErCtxMenu = {
	clientX: number
	clientY: number
}

export type EdgeSpine = {
	lane: number
	spineX: number
	yNudge: number
}

export type LiveErLoadResult = {
	diagram: ErDiagramState
	title: string
	summary: string
	focusNodeId: string
}

/** Table that roots a live ER tab (that table + directly related tables). */
export type LiveErFocus = {
	schema?: string | null
	name: string
}
