/**
 * ER canvas layout shared by live catalog load and the designer.
 *
 * Geometry must match `erConstants` BASE sizes (scale 1). The old packer
 * used each node's own height as the *row stride*, so a short table in
 * row 2 sat inside a tall table from row 1.
 *
 * SQL / FK graphs: parents left, children right (edge.from = child).
 * Isolated nodes (Mongo collections, Redis keys, tables with no FKs):
 * compact name-sorted grid using the *max* height of each row.
 */
import type { ErDiagramState, ErEdge, ErNode } from '$lib/types/er'
import { ER_GRID, snapToGrid } from '$lib/types/er'
import {
	ER_HEADER_H_BASE,
	ER_ROW_H_BASE,
	ER_TABLE_W_BASE,
} from './erConstants'

export const ER_TABLE_W = ER_TABLE_W_BASE
export const ER_HEADER_H = ER_HEADER_H_BASE
export const ER_ROW_H = ER_ROW_H_BASE
/** Wide enough for several non-overlapping FK spines between layout columns. */
export const ER_GAP_X = ER_GRID * 5
export const ER_GAP_Y = ER_GRID * 3
export const ER_PAD = ER_GRID * 2

/** Wrap a relationship layer once it grows taller than this. */
const MAX_LAYER_H = 2000

type Geom = {
	tableW: number
	headerH: number
	rowH: number
	gapX: number
	gapY: number
	pad: number
	maxColH: number
}

function geomForScale(scale: number): Geom {
	const s = Number.isFinite(scale) && scale > 0 ? scale : 1
	return {
		tableW: Math.round(ER_TABLE_W * s),
		headerH: Math.round(ER_HEADER_H * s),
		rowH: Math.round(ER_ROW_H * s),
		gapX: Math.round(ER_GAP_X * s),
		gapY: Math.round(ER_GAP_Y * s),
		pad: Math.round(ER_PAD * s),
		maxColH: Math.round(MAX_LAYER_H * Math.max(1, s)),
	}
}

export function erNodeHeight(columnCount: number, scale = 1): number {
	const g = geomForScale(scale)
	return g.headerH + Math.max(1, columnCount) * g.rowH
}

function heightOf(node: ErNode, g: Geom): number {
	return g.headerH + Math.max(1, node.columns.length) * g.rowH
}

function avg(values: number[]): number {
	if (values.length === 0) return 0
	return values.reduce((a, b) => a + b, 0) / values.length
}

/**
 * Place a list as vertical stacks that wrap into extra columns.
 * Returns the x where the next block may start (right edge + gap).
 */
function packStack(
	list: ErNode[],
	startX: number,
	startY: number,
	g: Geom,
	pos: Map<string, { x: number; y: number }>,
): number {
	if (list.length === 0) return startX
	let colX = startX
	let y = startY
	let used = 0
	let maxRight = startX
	for (const node of list) {
		const h = heightOf(node, g)
		if (used > 0 && y + h > g.pad + g.maxColH) {
			colX += g.tableW + g.gapX
			y = startY
			used = 0
		}
		pos.set(node.id, { x: snapToGrid(colX), y: snapToGrid(y) })
		y += h + g.gapY
		used += 1
		maxRight = Math.max(maxRight, colX + g.tableW)
	}
	return maxRight + g.gapX
}

/**
 * Compact grid: every cell in a row uses that row's max table height
 * so a short table never slides under a tall neighbor.
 */
function packGrid(
	list: ErNode[],
	startX: number,
	startY: number,
	g: Geom,
	pos: Map<string, { x: number; y: number }>,
): number {
	if (list.length === 0) return startX
	const cols = Math.max(1, Math.ceil(Math.sqrt(list.length)))
	const rows = Math.ceil(list.length / cols)
	const rowMax: number[] = []
	for (let r = 0; r < rows; r++) {
		let mh = 0
		for (let c = 0; c < cols; c++) {
			const n = list[r * cols + c]
			if (n) mh = Math.max(mh, heightOf(n, g))
		}
		rowMax.push(mh)
	}
	const rowY: number[] = []
	let y = startY
	for (const h of rowMax) {
		rowY.push(y)
		y += h + g.gapY
	}
	list.forEach((node, i) => {
		const c = i % cols
		const r = Math.floor(i / cols)
		pos.set(node.id, {
			x: snapToGrid(startX + c * (g.tableW + g.gapX)),
			y: snapToGrid(rowY[r] ?? startY),
		})
	})
	return startX + cols * (g.tableW + g.gapX)
}

/**
 * Longest-path layering: a table that references no one sits in layer 0
 * (left). Children that point at it land to the right. Cycles collapse
 * into the current layer so layout always terminates.
 */
function assignLayers(
	group: ErNode[],
	parentsOf: Map<string, Set<string>>,
): Map<string, number> {
	const layerOf = new Map<string, number>()
	const remaining = new Set(group.map((n) => n.id))
	let layer = 0
	let guard = 0
	while (remaining.size > 0 && guard++ < group.length + 2) {
		const ready: string[] = []
		for (const id of remaining) {
			const stillNeeded = [...(parentsOf.get(id) ?? [])].filter((p) =>
				remaining.has(p),
			)
			if (stillNeeded.length === 0) ready.push(id)
		}
		const take = ready.length > 0 ? ready : [...remaining]
		for (const id of take) {
			layerOf.set(id, layer)
			remaining.delete(id)
		}
		layer += 1
	}
	return layerOf
}

export function layoutErNodes(
	nodes: ErNode[],
	edges: ErEdge[],
	opts?: { scale?: number },
): ErNode[] {
	if (nodes.length === 0) return nodes
	const g = geomForScale(opts?.scale ?? 1)

	const ids = new Set(nodes.map((n) => n.id))
	const parentsOf = new Map<string, Set<string>>()
	const childrenOf = new Map<string, Set<string>>()
	for (const n of nodes) {
		parentsOf.set(n.id, new Set())
		childrenOf.set(n.id, new Set())
	}
	for (const e of edges) {
		if (!ids.has(e.from) || !ids.has(e.to) || e.from === e.to) continue
		parentsOf.get(e.from)!.add(e.to)
		childrenOf.get(e.to)!.add(e.from)
	}

	const connected: ErNode[] = []
	const isolated: ErNode[] = []
	for (const n of nodes) {
		const degree =
			(parentsOf.get(n.id)?.size ?? 0) + (childrenOf.get(n.id)?.size ?? 0)
		if (degree > 0) connected.push(n)
		else isolated.push(n)
	}

	const pos = new Map<string, { x: number; y: number }>()
	let nextX = g.pad

	if (connected.length > 0) {
		const layerOf = assignLayers(connected, parentsOf)
		const byLayer = new Map<number, ErNode[]>()
		let maxLayer = 0
		for (const n of connected) {
			const L = layerOf.get(n.id) ?? 0
			maxLayer = Math.max(maxLayer, L)
			const arr = byLayer.get(L) ?? []
			arr.push(n)
			byLayer.set(L, arr)
		}

		for (let L = 0; L <= maxLayer; L++) {
			const list = byLayer.get(L) ?? []
			if (L === 0) {
				list.sort((a, b) => a.name.localeCompare(b.name))
			} else {
				list.sort((a, b) => {
					const ya = avg(
						[...(parentsOf.get(a.id) ?? [])].map((id) => pos.get(id)?.y ?? 0),
					)
					const yb = avg(
						[...(parentsOf.get(b.id) ?? [])].map((id) => pos.get(id)?.y ?? 0),
					)
					if (ya !== yb) return ya - yb
					return a.name.localeCompare(b.name)
				})
			}
			nextX = packStack(list, nextX, g.pad, g, pos)
		}
	}

	isolated.sort((a, b) => a.name.localeCompare(b.name))
	packGrid(isolated, connected.length > 0 ? nextX : g.pad, g.pad, g, pos)

	return nodes.map((n) => {
		const p = pos.get(n.id)
		return p ? { ...n, x: p.x, y: p.y } : n
	})
}

export function layoutErDiagram(
	diagram: ErDiagramState,
	opts?: { scale?: number },
): ErDiagramState {
	return {
		...diagram,
		nodes: layoutErNodes(diagram.nodes, diagram.edges, opts),
	}
}

/** Shift a packed layout so its bounding box sits in the view center. */
export function placeErNodesInView(
	nodes: ErNode[],
	opts: { viewW: number; viewH: number; zoom?: number; scale?: number },
): { nodes: ErNode[]; scrollLeft: number; scrollTop: number } {
	if (nodes.length === 0 || opts.viewW < 8 || opts.viewH < 8) {
		return { nodes, scrollLeft: 0, scrollTop: 0 }
	}
	const zoom = opts.zoom && opts.zoom > 0 ? opts.zoom : 1
	const g = geomForScale(opts.scale ?? 1)
	let minX = Infinity
	let minY = Infinity
	let maxX = -Infinity
	let maxY = -Infinity
	for (const n of nodes) {
		const h = heightOf(n, g)
		minX = Math.min(minX, n.x)
		minY = Math.min(minY, n.y)
		maxX = Math.max(maxX, n.x + g.tableW)
		maxY = Math.max(maxY, n.y + h)
	}
	const cx = (minX + maxX) / 2
	const cy = (minY + maxY) / 2
	let dx = 0
	let dy = 0
	const rawScrollLeft = cx * zoom - opts.viewW / 2
	const rawScrollTop = cy * zoom - opts.viewH / 2
	if (rawScrollLeft < 0) dx = -rawScrollLeft / zoom
	if (rawScrollTop < 0) dy = -rawScrollTop / zoom
	dx = snapToGrid(dx)
	dy = snapToGrid(dy)
	const next =
		Math.abs(dx) < 0.5 && Math.abs(dy) < 0.5
			? nodes
			: nodes.map((n) => ({ ...n, x: n.x + dx, y: n.y + dy }))
	return {
		nodes: next,
		scrollLeft: Math.max(0, Math.round((cx + dx) * zoom - opts.viewW / 2)),
		scrollTop: Math.max(0, Math.round((cy + dy) * zoom - opts.viewH / 2)),
	}
}

export function erContentBounds(
	nodes: ErNode[],
	scale = 1,
): { width: number; height: number } {
	const g = geomForScale(scale)
	if (nodes.length === 0) {
		return { width: 1400, height: 800 }
	}
	let maxX = 400
	let maxY = 300
	for (const n of nodes) {
		const h = heightOf(n, g)
		maxX = Math.max(maxX, n.x + g.tableW + g.pad)
		maxY = Math.max(maxY, n.y + h + g.pad)
	}
	return {
		width: Math.max(1400, Math.ceil(maxX)),
		height: Math.max(800, Math.ceil(maxY)),
	}
}
