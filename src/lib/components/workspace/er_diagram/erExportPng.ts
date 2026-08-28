/**
 * Rasterize the live ER canvas to a PNG of the current layout.
 * The clone drops the grid; crop follows table cards and relationship paths.
 */
import type { ErNode } from '$lib/types/er'
import { toCanvas } from 'html-to-image'

export const ER_EXPORT_PAD = 40
const EXPORT_PIXEL_RATIO = 2

export type ErExportCrop = {
	x: number
	y: number
	width: number
	height: number
}

export function erNodeBox(
	node: ErNode,
	tableW: number,
	headerH: number,
	rowH: number,
): { x: number; y: number; width: number; height: number } {
	return {
		x: node.x,
		y: node.y,
		width: tableW,
		height: headerH + Math.max(1, node.columns.length) * rowH,
	}
}

/** Tight crop around tables (and an optional SVG/path union) plus padding. */
export function erExportCropRect(
	nodes: ErNode[],
	tableW: number,
	headerH: number,
	rowH: number,
	extra?: { x: number; y: number; width: number; height: number } | null,
	pad = ER_EXPORT_PAD,
): ErExportCrop | null {
	if (nodes.length === 0) return null
	let minX = Infinity
	let minY = Infinity
	let maxX = -Infinity
	let maxY = -Infinity
	for (const n of nodes) {
		const box = erNodeBox(n, tableW, headerH, rowH)
		minX = Math.min(minX, box.x)
		minY = Math.min(minY, box.y)
		maxX = Math.max(maxX, box.x + box.width)
		maxY = Math.max(maxY, box.y + box.height)
	}
	if (extra && extra.width > 0 && extra.height > 0) {
		minX = Math.min(minX, extra.x)
		minY = Math.min(minY, extra.y)
		maxX = Math.max(maxX, extra.x + extra.width)
		maxY = Math.max(maxY, extra.y + extra.height)
	}
	minX = Math.floor(minX - pad)
	minY = Math.floor(minY - pad)
	maxX = Math.ceil(maxX + pad)
	maxY = Math.ceil(maxY + pad)
	return {
		x: minX,
		y: minY,
		width: Math.max(1, maxX - minX),
		height: Math.max(1, maxY - minY),
	}
}

export function clampErExportCrop(
	crop: ErExportCrop,
	layerW: number,
	layerH: number,
): ErExportCrop {
	const x = Math.min(Math.max(0, crop.x), Math.max(0, layerW - 1))
	const y = Math.min(Math.max(0, crop.y), Math.max(0, layerH - 1))
	const width = Math.max(1, Math.min(crop.width, layerW - x))
	const height = Math.max(1, Math.min(crop.height, layerH - y))
	return { x, y, width, height }
}

function svgContentBox(
	root: HTMLElement,
): { x: number; y: number; width: number; height: number } | null {
	const svg = root.querySelector('svg')
	if (!svg) return null
	try {
		const b = svg.getBBox()
		if (!Number.isFinite(b.width) || b.width <= 0 || b.height <= 0) return null
		return { x: b.x, y: b.y, width: b.width, height: b.height }
	} catch {
		return null
	}
}

function canvasBackground(el: HTMLElement): string {
	const raw = getComputedStyle(el).getPropertyValue('--bg-app').trim()
	if (raw) return raw
	const wash = getComputedStyle(el).backgroundColor
	return wash && wash !== 'rgba(0, 0, 0, 0)' && wash !== 'transparent'
		? wash
		: '#131210'
}

function canvasToPngBytes(canvas: HTMLCanvasElement): Promise<Uint8Array> {
	return new Promise((resolve, reject) => {
		canvas.toBlob((blob) => {
			if (!blob) {
				reject(new Error('Could not encode PNG.'))
				return
			}
			void blob
				.arrayBuffer()
				.then((buf) => resolve(new Uint8Array(buf)))
				.catch(reject)
		}, 'image/png')
	})
}

function pointAt(path: SVGPathElement, t: number): { x: number; y: number } {
	const len = path.getTotalLength()
	const p = path.getPointAtLength(Math.max(0, Math.min(len, t)))
	return { x: p.x, y: p.y }
}

function appendArrowHead(
	path: SVGPathElement,
	from: { x: number; y: number },
	to: { x: number; y: number },
	fill: string,
) {
	const angle = Math.atan2(to.y - from.y, to.x - from.x)
	const size = 11
	const wing = size * 0.42
	const leftX = to.x - size * Math.cos(angle) + wing * Math.sin(angle)
	const leftY = to.y - size * Math.sin(angle) - wing * Math.cos(angle)
	const rightX = to.x - size * Math.cos(angle) - wing * Math.sin(angle)
	const rightY = to.y - size * Math.sin(angle) + wing * Math.cos(angle)
	const poly = document.createElementNS('http://www.w3.org/2000/svg', 'polygon')
	poly.setAttribute(
		'points',
		`${to.x},${to.y} ${leftX},${leftY} ${rightX},${rightY}`,
	)
	poly.setAttribute('fill', fill)
	path.parentNode?.appendChild(poly)
}

function appendTailDot(
	path: SVGPathElement,
	at: { x: number; y: number },
	fill: string,
) {
	const circle = document.createElementNS(
		'http://www.w3.org/2000/svg',
		'circle',
	)
	circle.setAttribute('cx', String(at.x))
	circle.setAttribute('cy', String(at.y))
	circle.setAttribute('r', '3.5')
	circle.setAttribute('fill', fill)
	path.parentNode?.appendChild(circle)
}

/** Markers (`url(#id)`) often vanish inside html-to-image's foreignObject. */
function inlineSvgMarkers(root: HTMLElement) {
	const stroke =
		getComputedStyle(root).getPropertyValue('--accent').trim() || '#2fbf96'
	const paths = root.querySelectorAll<SVGPathElement>('path')
	for (const path of paths) {
		const end = path.getAttribute('marker-end')
		const start = path.getAttribute('marker-start')
		if (!end && !start) continue
		const len = path.getTotalLength()
		if (!Number.isFinite(len) || len < 1) {
			path.removeAttribute('marker-end')
			path.removeAttribute('marker-start')
			continue
		}
		const computed = getComputedStyle(path).stroke
		const color =
			computed && computed !== 'none' && computed !== 'transparent'
				? computed
				: path.getAttribute('stroke') || stroke
		if (end && end !== 'none') {
			appendArrowHead(path, pointAt(path, len - 8), pointAt(path, len), color)
		}
		if (start && start !== 'none') {
			appendTailDot(path, pointAt(path, 0), color)
		}
		path.removeAttribute('marker-end')
		path.removeAttribute('marker-start')
	}
	root.querySelectorAll('defs').forEach((el) => el.remove())
}

function prepareExportClone(root: HTMLElement, layerW: number, layerH: number) {
	const host = document.createElement('div')
	host.setAttribute('data-er-export-host', '')
	host.style.cssText =
		'position:fixed;left:-100000px;top:0;pointer-events:none;z-index:-1;'
	const clone = root.cloneNode(true) as HTMLElement
	clone.style.transform = 'none'
	clone.style.transformOrigin = 'top left'
	clone.style.backgroundImage = 'none'
	clone.style.width = `${layerW}px`
	clone.style.height = `${layerH}px`
	clone.querySelectorAll('[data-er-export-hide]').forEach((el) => el.remove())
	// Selection / focus chrome is editor state, not part of the schema picture.
	clone.querySelectorAll<HTMLElement>('[data-er-table]').forEach((el) => {
		el.classList.remove(
			'border-accent',
			'ring-1',
			'ring-accent/25',
			'opacity-25',
		)
		el.classList.add('border-line')
		el.style.opacity = '1'
	})
	host.appendChild(clone)
	document.body.appendChild(host)
	inlineSvgMarkers(clone)
	return { host, clone }
}

/**
 * Snapshot `root` (the ER world layer) at the current card positions.
 * Grid, zoom transform, and `[data-er-export-hide]` chrome are omitted.
 */
export async function captureErDiagramPng(opts: {
	root: HTMLElement
	nodes: ErNode[]
	tableW: number
	headerH: number
	rowH: number
}): Promise<Uint8Array> {
	const { root, nodes, tableW, headerH, rowH } = opts
	const rawCrop = erExportCropRect(
		nodes,
		tableW,
		headerH,
		rowH,
		svgContentBox(root),
	)
	if (!rawCrop) throw new Error('Nothing to export — add a table first.')

	const layerW = Math.max(1, Math.round(root.offsetWidth || rawCrop.width))
	const layerH = Math.max(1, Math.round(root.offsetHeight || rawCrop.height))
	const crop = clampErExportCrop(rawCrop, layerW, layerH)
	const bg = canvasBackground(root)
	const { host, clone } = prepareExportClone(root, layerW, layerH)

	try {
		const full = await toCanvas(clone, {
			pixelRatio: EXPORT_PIXEL_RATIO,
			backgroundColor: bg,
			width: layerW,
			height: layerH,
			skipFonts: true,
			style: {
				transform: 'none',
				transformOrigin: 'top left',
				backgroundImage: 'none',
				backgroundColor: bg,
			},
		})

		const sx = Math.round(crop.x * EXPORT_PIXEL_RATIO)
		const sy = Math.round(crop.y * EXPORT_PIXEL_RATIO)
		const sw = Math.max(1, Math.round(crop.width * EXPORT_PIXEL_RATIO))
		const sh = Math.max(1, Math.round(crop.height * EXPORT_PIXEL_RATIO))

		const out = document.createElement('canvas')
		out.width = sw
		out.height = sh
		const ctx = out.getContext('2d')
		if (!ctx) throw new Error('Could not create export canvas.')
		ctx.fillStyle = bg
		ctx.fillRect(0, 0, sw, sh)
		ctx.drawImage(full, sx, sy, sw, sh, 0, 0, sw, sh)
		return canvasToPngBytes(out)
	} finally {
		host.remove()
	}
}
