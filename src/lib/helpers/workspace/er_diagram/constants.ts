import type { ErDiagramState, ErEdge, ErEdgeKind, ErNode } from '$lib/types/er'

/** Base table chrome at UI scale 1. Must match `erLayout` packer sizes. */
export const ER_TABLE_W_BASE = 280
export const ER_HEADER_H_BASE = 40
export const ER_ROW_H_BASE = 34
export const ER_ZOOM_MIN = 0.25
export const ER_ZOOM_MAX = 2
export const ER_ZOOM_STOPS = [0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.25, 1.5, 1.75, 2]

export type ErFocusedLink = {
	edge: ErEdge
	tableIds: Set<string>
	colIds: Set<string>
}

export function clampErZoom(z: number): number {
	return Math.min(ER_ZOOM_MAX, Math.max(ER_ZOOM_MIN, z))
}

export function nearestErZoomStop(current: number, dir: 1 | -1): number {
	if (dir > 0) {
		return ER_ZOOM_STOPS.find((s) => s > current + 0.001) ?? ER_ZOOM_MAX
	}
	const prev = [...ER_ZOOM_STOPS].reverse().find((s) => s < current - 0.001)
	return prev ?? ER_ZOOM_MIN
}

export function isBrokenEdge(kind: ErEdgeKind | string): boolean {
	return kind === 'broken' || kind === 'inferred' || kind === 'logical'
}

export function cloneDiagram<T>(d: T): T {
	return JSON.parse(JSON.stringify(d)) as T
}

export function isEditableTarget(t: EventTarget | null): boolean {
	if (!(t instanceof HTMLElement)) return false
	const tag = t.tagName
	if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true
	if (t.isContentEditable) return true
	return Boolean(t.closest("input, textarea, select, [contenteditable='true']"))
}

export function erNodeKindShort(kind: ErNode['kind']): string {
	if (kind === 'collection') return 'Coll'
	if (kind === 'key_pattern') return 'Key'
	return 'Table'
}

export function erFkColumnIds(edges: ErEdge[]): Set<string> {
	const ids = new Set<string>()
	for (const e of edges) {
		ids.add(e.fromColumnId)
		ids.add(e.toColumnId)
	}
	return ids
}

export function erFocusedLink(
	diagram: ErDiagramState | null,
	selectedEdgeId: string | null,
): ErFocusedLink | null {
	if (!diagram || !selectedEdgeId) return null
	const edge = diagram.edges.find((e) => e.id === selectedEdgeId)
	if (!edge) return null
	return {
		edge,
		tableIds: new Set([edge.from, edge.to]),
		colIds: new Set([edge.fromColumnId, edge.toColumnId]),
	}
}

export function erEdgesDrawOrder(
	edges: ErEdge[],
	selectedEdgeId: string | null,
): ErEdge[] {
	if (!selectedEdgeId) return edges
	const rest = edges.filter((e) => e.id !== selectedEdgeId)
	const sel = edges.find((e) => e.id === selectedEdgeId)
	return sel ? [...rest, sel] : edges
}
