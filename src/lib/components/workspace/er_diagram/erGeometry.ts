/**
 * Canvas geometry for ER tables and Manhattan (90°) relationship paths.
 * Sizes are already UI-scaled; callers pass `tableW` / `headerH` / `rowH`.
 */
import type { ErDiagramState, ErEdge, ErNode, ErPoint } from '$lib/types/er'
import { ER_GRID, snapToGrid } from '$lib/types/er'

export type TableSide = 'left' | 'right'

export type EdgeSpine = {
	lane: number
	spineX: number
	yNudge: number
}

export type ErGeomSizes = {
	tableW: number
	headerH: number
	rowH: number
}

type SpineDraft = {
	id: string
	kind: 'mid' | 'outer-left' | 'outer-right' | 'horiz' | 'self'
	start: ErPoint
	end: ErPoint
	channel: string
	y0: number
	y1: number
	left: number
	right: number
}

export function createErGeometry(sizes: ErGeomSizes) {
	const { tableW, headerH, rowH } = sizes

	function columnRowY(node: ErNode, columnId: string): number {
		const idx = Math.max(
			0,
			node.columns.findIndex((c) => c.id === columnId),
		)
		return node.y + headerH + idx * rowH + rowH / 2
	}

	function columnCenter(node: ErNode, columnId: string): ErPoint {
		return {
			x: node.x + tableW / 2,
			y: columnRowY(node, columnId),
		}
	}

	function columnSideAnchor(
		node: ErNode,
		columnId: string,
		side: TableSide,
	): ErPoint {
		const pad = 4
		return {
			x: side === 'left' ? node.x - pad : node.x + tableW + pad,
			y: columnRowY(node, columnId),
		}
	}

	function sideToward(node: ErNode, towardX: number): TableSide {
		const mid = node.x + tableW / 2
		return towardX >= mid ? 'right' : 'left'
	}

	function tableMidY(node: ErNode): number {
		return node.y + headerH + (node.columns.length * rowH) / 2
	}

	function isVerticalStack(a: ErNode, b: ErNode): boolean {
		const aMidX = a.x + tableW / 2
		const bMidX = b.x + tableW / 2
		const dx = Math.abs(aMidX - bMidX)
		const dy = Math.abs(tableMidY(a) - tableMidY(b))
		const hOverlap = Math.min(a.x + tableW, b.x + tableW) - Math.max(a.x, b.x)
		return hOverlap > tableW * 0.2 || (dy > dx && dy > ER_GRID)
	}

	function pickStackedOuterSide(a: ErNode, b: ErNode): TableSide {
		const maxRight = Math.max(a.x + tableW, b.x + tableW)
		const minLeft = Math.min(a.x, b.x)
		const rightClearance = maxRight - Math.min(a.x + tableW, b.x + tableW)
		const leftClearance = Math.max(a.x, b.x) - minLeft
		if (leftClearance > rightClearance + ER_GRID * 2) return 'left'
		return 'right'
	}

	function pickLinkSides(
		a: ErNode,
		b: ErNode,
		wps: ErPoint[],
	): { fromSide: TableSide; toSide: TableSide } {
		if (a.id === b.id) {
			return { fromSide: 'right', toSide: 'right' }
		}
		if (wps.length > 0) {
			return {
				fromSide: sideToward(a, wps[0]!.x),
				toSide: sideToward(b, wps[wps.length - 1]!.x),
			}
		}
		if (isVerticalStack(a, b)) {
			const side = pickStackedOuterSide(a, b)
			return { fromSide: side, toSide: side }
		}
		const aMid = a.x + tableW / 2
		const bMid = b.x + tableW / 2
		if (bMid >= aMid) {
			return { fromSide: 'right', toSide: 'left' }
		}
		return { fromSide: 'left', toSide: 'right' }
	}

	function samePoint(a: ErPoint, b: ErPoint, eps = 0.5): boolean {
		return Math.abs(a.x - b.x) < eps && Math.abs(a.y - b.y) < eps
	}

	function toOrthogonal(pts: ErPoint[]): ErPoint[] {
		if (pts.length === 0) return []
		const out: ErPoint[] = [{ x: pts[0]!.x, y: pts[0]!.y }]
		for (let i = 1; i < pts.length; i++) {
			const prev = out[out.length - 1]!
			const cur = pts[i]!
			if (samePoint(prev, cur)) continue
			if (prev.x !== cur.x && prev.y !== cur.y) {
				const elbow = { x: cur.x, y: prev.y }
				if (!samePoint(prev, elbow)) out.push(elbow)
			}
			const last = out[out.length - 1]!
			if (!samePoint(last, cur)) out.push({ x: cur.x, y: cur.y })
		}
		return out
	}

	function orthoGridSnapFrom(prev: ErPoint, raw: ErPoint): ErPoint {
		const gx = snapToGrid(raw.x)
		const gy = snapToGrid(raw.y)
		const dx = Math.abs(gx - prev.x)
		const dy = Math.abs(gy - prev.y)
		if (dx === 0 && dy === 0) return { x: prev.x, y: prev.y }
		if (dx >= dy) return { x: gx, y: prev.y }
		return { x: prev.x, y: gy }
	}

	function defaultManhattanWaypoints(
		start: ErPoint,
		end: ErPoint,
		fromSide: TableSide,
		toSide: TableSide,
		spine?: EdgeSpine,
	): ErPoint[] {
		const yNudge = spine?.yNudge ?? 0

		if (Math.abs(start.y - end.y) < 0.5) {
			if (Math.abs(yNudge) < 0.5) return []
			const mid = spine?.spineX ?? (start.x + end.x) / 2
			return [
				{ x: mid, y: start.y },
				{ x: mid, y: start.y + yNudge },
				{
					x: end.x > start.x ? end.x - ER_GRID : end.x + ER_GRID,
					y: end.y + yNudge,
				},
			]
		}

		const spineX =
			spine?.spineX ??
			(fromSide === toSide
				? fromSide === 'right'
					? Math.max(start.x, end.x) + ER_GRID * 2
					: Math.min(start.x, end.x) - ER_GRID * 2
				: (start.x + end.x) / 2)

		return [
			{ x: spineX, y: start.y },
			{ x: spineX, y: end.y },
		]
	}

	function ensureHorizontalPorts(
		pts: ErPoint[],
		fromSide: TableSide,
		toSide: TableSide,
		lane = 0,
	): ErPoint[] {
		if (pts.length < 2) return pts
		let out = pts.map((p) => ({ x: p.x, y: p.y }))
		const pad = ER_GRID * 2 + lane * ER_GRID

		const s0 = out[0]!
		const s1 = out[1]!
		if (Math.abs(s0.x - s1.x) < 0.5 && Math.abs(s0.y - s1.y) > 0.5) {
			const cx =
				fromSide === 'right' ? snapToGrid(s0.x + pad) : snapToGrid(s0.x - pad)
			out = [s0, { x: cx, y: s0.y }, { x: cx, y: s1.y }, ...out.slice(2)]
		}

		if (out.length >= 2) {
			const e0 = out[out.length - 2]!
			const e1 = out[out.length - 1]!
			if (Math.abs(e0.x - e1.x) < 0.5 && Math.abs(e0.y - e1.y) > 0.5) {
				const cx =
					toSide === 'right' ? snapToGrid(e1.x + pad) : snapToGrid(e1.x - pad)
				out = [...out.slice(0, -1), { x: cx, y: e0.y }, { x: cx, y: e1.y }, e1]
			}
		}

		return toOrthogonal(out)
	}

	function autoSelfLoopWaypoints(
		node: ErNode,
		fromColumnId: string,
		toColumnId: string,
	): ErPoint[] {
		const y0 = columnRowY(node, fromColumnId)
		const y1 = columnRowY(node, toColumnId)
		const loopX = snapToGrid(node.x + tableW + ER_GRID * 3)
		return [
			{ x: loopX, y: y0 },
			{ x: loopX, y: y1 },
		]
	}

	function edgeAnchorsAndWaypoints(
		diagram: ErDiagramState,
		edge: ErEdge,
	): {
		start: ErPoint
		end: ErPoint
		wps: ErPoint[]
		fromSide: TableSide
		toSide: TableSide
	} | null {
		const a = diagram.nodes.find((n) => n.id === edge.from)
		const b = diagram.nodes.find((n) => n.id === edge.to)
		if (!a || !b) return null
		const wps = [...(edge.waypoints ?? [])]
		const { fromSide, toSide } = pickLinkSides(a, b, wps)
		const start = columnSideAnchor(a, edge.fromColumnId, fromSide)
		const end = columnSideAnchor(b, edge.toColumnId, toSide)
		return { start, end, wps, fromSide, toSide }
	}

	function spineFromLane(d: SpineDraft, lane: number, sep: number): EdgeSpine {
		if (d.kind === 'horiz') {
			const sign = lane % 2 === 0 ? 1 : -1
			const mag = Math.ceil((lane + 1) / 2) * 6
			return { lane, spineX: (d.left + d.right) / 2, yNudge: sign * mag }
		}
		if (d.kind === 'self') {
			return {
				lane,
				spineX: d.right + ER_GRID * 3 + lane * sep,
				yNudge: 0,
			}
		}
		if (d.kind === 'outer-right') {
			return { lane, spineX: d.right + ER_GRID * 2 + lane * sep, yNudge: 0 }
		}
		if (d.kind === 'outer-left') {
			return { lane, spineX: d.left - ER_GRID * 2 - lane * sep, yNudge: 0 }
		}

		const gap = d.right - d.left
		const inset = 8
		const usable = Math.max(sep, gap - inset * 2)
		const tracks = Math.max(1, Math.floor(usable / sep))
		if (lane < tracks) {
			return {
				lane,
				spineX: d.left + inset + (lane + 0.5) * (usable / tracks),
				yNudge: 0,
			}
		}
		const over = lane - tracks
		const extra = ER_GRID * 2 + Math.floor(over / 2) * sep
		return {
			lane,
			spineX: over % 2 === 0 ? d.right + extra : d.left - extra,
			yNudge: 0,
		}
	}

	function assignEdgeSpines(diagram: ErDiagramState): Map<string, EdgeSpine> {
		const out = new Map<string, EdgeSpine>()
		const drafts: SpineDraft[] = []
		for (const edge of diagram.edges) {
			const raw = edgeAnchorsAndWaypoints(diagram, edge)
			if (!raw) continue
			const { start, end, fromSide, toSide } = raw
			const a = diagram.nodes.find((n) => n.id === edge.from)
			const b = diagram.nodes.find((n) => n.id === edge.to)
			if (!a || !b) continue
			const y0 = Math.min(start.y, end.y)
			const y1 = Math.max(start.y, end.y)
			const left = Math.min(start.x, end.x)
			const right = Math.max(start.x, end.x)
			if (a.id === b.id) {
				drafts.push({
					id: edge.id,
					kind: 'self',
					start,
					end,
					channel: `self:${a.id}`,
					y0,
					y1,
					left,
					right,
				})
				continue
			}
			if (Math.abs(start.y - end.y) < 0.5) {
				const pair = [edge.from, edge.to].sort().join('>')
				drafts.push({
					id: edge.id,
					kind: 'horiz',
					start,
					end,
					channel: `h:${Math.round(start.y / 4)}:${pair}`,
					y0,
					y1,
					left,
					right,
				})
				continue
			}
			if (fromSide === toSide) {
				const kind = fromSide === 'right' ? 'outer-right' : 'outer-left'
				drafts.push({
					id: edge.id,
					kind,
					start,
					end,
					channel: `${kind}:${Math.round((fromSide === 'right' ? right : left) / ER_GRID)}`,
					y0,
					y1,
					left,
					right,
				})
				continue
			}
			drafts.push({
				id: edge.id,
				kind: 'mid',
				start,
				end,
				channel: `mid:${Math.round(left / ER_GRID)}:${Math.round(right / ER_GRID)}`,
				y0,
				y1,
				left,
				right,
			})
		}

		const byChannel = new Map<string, SpineDraft[]>()
		for (const d of drafts) {
			const arr = byChannel.get(d.channel) ?? []
			arr.push(d)
			byChannel.set(d.channel, arr)
		}

		const yPad = Math.max(10, rowH * 0.4)
		const sep = 14

		for (const group of byChannel.values()) {
			group.sort((a, b) => a.y0 - b.y0 || a.id.localeCompare(b.id))
			const laneOf = new Map<string, number>()
			const active: { lane: number; y1: number }[] = []
			for (const d of group) {
				const used = new Set(
					active.filter((a) => a.y1 + yPad > d.y0).map((a) => a.lane),
				)
				let lane = 0
				while (used.has(lane)) lane += 1
				laneOf.set(d.id, lane)
				active.push({ lane, y1: d.y1 })
			}

			for (const d of group) {
				const lane = laneOf.get(d.id) ?? 0
				out.set(d.id, spineFromLane(d, lane, sep))
			}
		}
		return out
	}

	function edgeSpine(
		diagram: ErDiagramState,
		spines: Map<string, EdgeSpine>,
		edge: ErEdge,
	): EdgeSpine {
		const hit = spines.get(edge.id)
		if (hit) return hit
		const raw = edgeAnchorsAndWaypoints(diagram, edge)
		if (!raw) return { lane: 0, spineX: 0, yNudge: 0 }
		return {
			lane: 0,
			spineX: (raw.start.x + raw.end.x) / 2,
			yNudge: 0,
		}
	}

	function edgePoints(
		diagram: ErDiagramState,
		spines: Map<string, EdgeSpine>,
		edge: ErEdge,
	): ErPoint[] {
		const raw = edgeAnchorsAndWaypoints(diagram, edge)
		if (!raw) return []
		const spine = edgeSpine(diagram, spines, edge)
		let { start, end, wps, fromSide, toSide } = raw

		if (wps.length === 0 || wps.length <= 2) {
			wps = defaultManhattanWaypoints(start, end, fromSide, toSide, spine)
		}

		const orth = toOrthogonal([start, ...wps, end])
		return ensureHorizontalPorts(orth, fromSide, toSide, spine.lane)
	}

	function normalizeEdgeWaypoints(
		diagram: ErDiagramState,
		spines: Map<string, EdgeSpine>,
		edge: ErEdge,
		wps: ErPoint[],
	): ErPoint[] {
		const a = diagram.nodes.find((n) => n.id === edge.from)
		const b = diagram.nodes.find((n) => n.id === edge.to)
		if (!a || !b) return wps
		const { fromSide, toSide } = pickLinkSides(a, b, wps)
		const start = columnSideAnchor(a, edge.fromColumnId, fromSide)
		const end = columnSideAnchor(b, edge.toColumnId, toSide)
		const lane = edgeSpine(diagram, spines, edge).lane
		const full = ensureHorizontalPorts(
			toOrthogonal([
				start,
				...wps.map((p) => ({ x: snapToGrid(p.x), y: snapToGrid(p.y) })),
				end,
			]),
			fromSide,
			toSide,
			lane,
		)
		if (full.length <= 2) return []
		return full.slice(1, -1)
	}

	function edgePolyline(
		diagram: ErDiagramState,
		spines: Map<string, EdgeSpine>,
		edge: ErEdge,
	): string {
		const pts = edgePoints(diagram, spines, edge)
		if (!pts.length) return ''
		return pts.map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x} ${p.y}`).join(' ')
	}

	function segmentMids(
		diagram: ErDiagramState,
		spines: Map<string, EdgeSpine>,
		edge: ErEdge,
	): { x: number; y: number; insertAt: number }[] {
		const pts = edgePoints(diagram, spines, edge)
		const mids: { x: number; y: number; insertAt: number }[] = []
		for (let i = 0; i < pts.length - 1; i++) {
			const a = pts[i]!
			const b = pts[i + 1]!
			if (samePoint(a, b)) continue
			mids.push({
				x: (a.x + b.x) / 2,
				y: (a.y + b.y) / 2,
				insertAt: Math.max(0, i),
			})
		}
		return mids
	}

	function linkDraftStart(
		diagram: ErDiagramState,
		linkFrom: { tableId: string; columnId: string },
		draftWaypoints: ErPoint[],
		cursorPt: ErPoint | null,
	): ErPoint | null {
		const a = diagram.nodes.find((n) => n.id === linkFrom.tableId)
		if (!a) return null
		const towardX =
			draftWaypoints[0]?.x ?? cursorPt?.x ?? a.x + tableW + ER_GRID
		return columnSideAnchor(a, linkFrom.columnId, sideToward(a, towardX))
	}

	function linkDraftLast(
		diagram: ErDiagramState,
		linkFrom: { tableId: string; columnId: string },
		draftWaypoints: ErPoint[],
		cursorPt: ErPoint | null,
	): ErPoint | null {
		if (draftWaypoints.length) return draftWaypoints[draftWaypoints.length - 1]!
		return linkDraftStart(diagram, linkFrom, draftWaypoints, cursorPt)
	}

	function previewPolyline(
		diagram: ErDiagramState,
		linkFrom: { tableId: string; columnId: string },
		draftWaypoints: ErPoint[],
		cursorPt: ErPoint,
	): string {
		const p0 = linkDraftStart(diagram, linkFrom, draftWaypoints, cursorPt)
		if (!p0) return ''
		const last = linkDraftLast(diagram, linkFrom, draftWaypoints, cursorPt) ?? p0
		const tip = orthoGridSnapFrom(last, cursorPt)
		const pts = toOrthogonal([p0, ...draftWaypoints, tip])
		return pts.map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x} ${p.y}`).join(' ')
	}

	return {
		tableW,
		headerH,
		rowH,
		columnRowY,
		columnCenter,
		columnSideAnchor,
		sideToward,
		tableMidY,
		isVerticalStack,
		pickStackedOuterSide,
		pickLinkSides,
		samePoint,
		toOrthogonal,
		orthoGridSnapFrom,
		defaultManhattanWaypoints,
		ensureHorizontalPorts,
		autoSelfLoopWaypoints,
		edgeAnchorsAndWaypoints,
		assignEdgeSpines,
		edgeSpine,
		edgePoints,
		normalizeEdgeWaypoints,
		edgePolyline,
		segmentMids,
		linkDraftStart,
		linkDraftLast,
		previewPolyline,
	}
}

export type ErGeometry = ReturnType<typeof createErGeometry>
