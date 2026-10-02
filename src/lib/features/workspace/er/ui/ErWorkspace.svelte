<script lang="ts">
	/**
	 * Live ER viewer — catalog schema, rearrange-only.
	 */
	import type { DataFamily } from '#core/contracts'
	import { ContextMenu } from '#core/ui'
	import { currentUiScale, exportPng, onUiScaleChange } from '#core/util'
	import { showToast } from '#features/toast'
	import { onMount, tick } from 'svelte'
	import {
		ER_HEADER_H_BASE,
		ER_ROW_H_BASE,
		ER_TABLE_W_BASE,
		ER_ZOOM_MAX,
		ER_ZOOM_MIN,
		FAMILY_META,
		clampErZoom,
		cloneDiagram,
		erEdgesDrawOrder,
		erFkColumnIds,
		erFocusedLink,
		isEditableTarget,
		nearestErZoomStop,
		snapToGrid,
	} from '../constants'
	import { erCtxMenuItems } from '../context-menu'
	import { diagramToDbml } from '../dbml'
	import { captureErDiagramPng } from '../export-png'
	import { createErGeometry } from '../geometry'
	import { erContentBounds, layoutErNodes, placeErNodesInView } from '../layout'
	import type { ErCtxMenu, ErDiagramState, ErPoint } from '../types'
	import ErCanvas from './ErCanvas.svelte'
	import ErDbmlSnapshot from './ErDbmlSnapshot.svelte'
	import ErStatusBar from './ErStatusBar.svelte'

	interface Props {
		family: DataFamily
		initialDiagram?: ErDiagramState | null
		initialName?: string
		/** Select this table when a live neighborhood diagram loads. */
		focusNodeId?: string | null
		/** Reload tables / FKs from the database. */
		onrefresh?: () => void
		refreshing?: boolean
	}

	let {
		family,
		initialDiagram = null,
		initialName = '',
		focusNodeId = null,
		onrefresh,
		refreshing = false,
	}: Props = $props()

	let uiScale = $state(typeof document !== 'undefined' ? currentUiScale() : 1)
	const TABLE_W = $derived(Math.round(ER_TABLE_W_BASE * uiScale))
	const HEADER_H = $derived(Math.round(ER_HEADER_H_BASE * uiScale))
	const ROW_H = $derived(Math.round(ER_ROW_H_BASE * uiScale))
	const geom = $derived(
		createErGeometry({ tableW: TABLE_W, headerH: HEADER_H, rowH: ROW_H }),
	)

	onMount(() => {
		uiScale = currentUiScale()
		return onUiScaleChange((s) => {
			uiScale = s
		})
	})

	let diagram = $state<ErDiagramState | null>(null)
	let diagramName = $state('Table diagram')
	let selectedTableIds = $state<string[]>([])
	let selectedEdgeId = $state<string | null>(null)
	let showGrid = $state(true)
	let showDbml = $state(false)
	let dbmlCopied = $state(false)
	let dbmlText = $state('')
	let dbmlBusy = $state(false)
	let dbmlStale = $state(true)
	let dbmlGen = 0
	let viewZoom = $state(1)

	let dragTableId = $state<string | null>(null)
	let dragPreview = $state<Record<string, { x: number; y: number }> | null>(
		null,
	)
	let dragOffX = 0
	let dragOffY = 0
	let didDrag = false
	let multiDragOrigins: Record<string, { x: number; y: number }> | null = null
	let dragRaf = 0
	let pendingDrag: Record<string, { x: number; y: number }> | null = null
	let canvasEl: HTMLDivElement | null = $state(null)
	let exportRoot: HTMLDivElement | null = $state(null)
	let exportingPng = $state(false)
	let ctxMenu = $state<ErCtxMenu | null>(null)

	const meta = $derived(FAMILY_META[family])
	const ctxMenuItems = $derived(
		erCtxMenuItems({
			canDownload: !!diagram && diagram.nodes.length > 0 && !exportingPng,
		}),
	)
	const entityWord = $derived(meta.nodeLabel.toLowerCase())
	const fkColumnIds = $derived(erFkColumnIds(diagram?.edges ?? []))
	const focusedLink = $derived(erFocusedLink(diagram, selectedEdgeId))
	const edgesDrawOrder = $derived(
		erEdgesDrawOrder(diagram?.edges ?? [], selectedEdgeId),
	)
	const displayDiagram = $derived.by(() => {
		if (!diagram) return null
		if (!dragPreview) return diagram
		return {
			...diagram,
			nodes: diagram.nodes.map((n) => {
				const p = dragPreview![n.id]
				return p ? { ...n, x: p.x, y: p.y } : n
			}),
		}
	})
	const edgeSpines = $derived.by(() =>
		displayDiagram ? geom.assignEdgeSpines(displayDiagram) : new Map(),
	)
	const canvasBounds = $derived.by(() =>
		erContentBounds(displayDiagram?.nodes ?? [], uiScale),
	)
	const zoomLabel = $derived(`${Math.round(viewZoom * 100)}%`)
	const canZoomOut = $derived(viewZoom > ER_ZOOM_MIN + 0.001)
	const canZoomIn = $derived(viewZoom < ER_ZOOM_MAX - 0.001)
	const hasNodes = $derived(!!diagram && diagram.nodes.length > 0)
	const selectionCount = $derived(selectedTableIds.length)

	let lastInitKey = ''

	$effect(() => {
		const f = family
		const live = initialDiagram
		const nameHint = initialName?.trim() || ''
		const liveKey = live
			? `live:${live.nodes.length}:${live.edges.length}:${nameHint}:${focusNodeId ?? ''}`
			: 'empty'
		const key = `${f}::${liveKey}`
		if (lastInitKey === key) return
		lastInitKey = key

		if (live && live.nodes) {
			const cloned = cloneDiagram({
				family: live.family || f,
				nodes: live.nodes,
				edges: live.edges ?? [],
			})
			diagram = {
				...cloned,
				nodes: layoutErNodes(cloned.nodes, cloned.edges, { scale: uiScale }),
			}
			diagramName = nameHint || 'Table diagram'
		} else {
			diagram = { family: f, nodes: [], edges: [] }
			diagramName = nameHint || 'Table diagram'
		}
		selectedTableIds =
			focusNodeId && live?.nodes.some((n) => n.id === focusNodeId)
				? [focusNodeId]
				: []
		selectedEdgeId = null
		ctxMenu = null
		showDbml = false
		dbmlText = ''
		dbmlBusy = false
		dbmlStale = true
		dbmlGen += 1
		viewZoom = 1
		dragPreview = null
		pendingDrag = null
		if (dragRaf) {
			cancelAnimationFrame(dragRaf)
			dragRaf = 0
		}
		requestCenterInCanvas()
	})

	let centerTries = 0

	function requestCenterInCanvas() {
		centerTries = 0
		void tick().then(() => {
			requestAnimationFrame(() => applyCenterInCanvas())
		})
	}

	function applyCenterInCanvas() {
		if (!diagram || diagram.nodes.length === 0) return
		const el = canvasEl
		if (!el || el.clientWidth < 8 || el.clientHeight < 8) {
			if (centerTries++ < 12) requestAnimationFrame(() => applyCenterInCanvas())
			return
		}
		centerTries = 0
		const placed = placeErNodesInView(diagram.nodes, {
			viewW: el.clientWidth,
			viewH: el.clientHeight,
			zoom: viewZoom,
			scale: uiScale,
		})
		diagram = { ...diagram, nodes: placed.nodes }
		void tick().then(() => {
			canvasEl?.scrollTo({ left: placed.scrollLeft, top: placed.scrollTop })
		})
	}

	function invalidateDbml() {
		dbmlStale = true
		if (showDbml) queueDbml()
	}

	function queueDbml() {
		if (!showDbml) return
		if (!dbmlStale && dbmlText) return
		const token = ++dbmlGen
		const snap = diagram
		const name = diagramName
		dbmlBusy = true
		requestAnimationFrame(() => {
			window.setTimeout(() => {
				if (token !== dbmlGen) return
				dbmlText = diagramToDbml(snap, name)
				dbmlStale = false
				dbmlBusy = false
			}, 0)
		})
	}

	function canvasPoint(e: {
		clientX: number
		clientY: number
	}): ErPoint | null {
		if (!canvasEl) return null
		const rect = canvasEl.getBoundingClientRect()
		const z = viewZoom || 1
		return {
			x: (e.clientX - rect.left + canvasEl.scrollLeft) / z,
			y: (e.clientY - rect.top + canvasEl.scrollTop) / z,
		}
	}

	let zoomWrite = 0

	async function setViewZoom(
		next: number,
		origin?: { clientX: number; clientY: number },
	) {
		const z = clampErZoom(next)
		if (Math.abs(z - viewZoom) < 0.0005) return
		const el = canvasEl
		let worldX = 0
		let worldY = 0
		let ox = 0
		let oy = 0
		if (el) {
			const rect = el.getBoundingClientRect()
			if (origin) {
				ox = origin.clientX - rect.left
				oy = origin.clientY - rect.top
			} else {
				ox = rect.width / 2
				oy = rect.height / 2
			}
			worldX = (ox + el.scrollLeft) / viewZoom
			worldY = (oy + el.scrollTop) / viewZoom
		}
		const write = ++zoomWrite
		viewZoom = z
		await tick()
		if (write !== zoomWrite || !el) return
		el.scrollLeft = worldX * z - ox
		el.scrollTop = worldY * z - oy
	}

	function zoomIn() {
		void setViewZoom(nearestErZoomStop(viewZoom, 1))
	}

	function zoomOut() {
		void setViewZoom(nearestErZoomStop(viewZoom, -1))
	}

	function resetZoom() {
		void setViewZoom(1)
	}

	$effect(() => {
		const el = canvasEl
		if (!el) return

		let raf = 0
		let pending: { z: number; clientX: number; clientY: number } | null = null
		let gestureBase = 1

		const flush = () => {
			raf = 0
			const p = pending
			pending = null
			if (p) void setViewZoom(p.z, { clientX: p.clientX, clientY: p.clientY })
		}

		const queueZoom = (z: number, clientX: number, clientY: number) => {
			pending = { z: clampErZoom(z), clientX, clientY }
			if (!raf) raf = requestAnimationFrame(flush)
		}

		const onWheel = (e: WheelEvent) => {
			if (!(e.ctrlKey || e.metaKey)) return
			e.preventDefault()
			let dy = e.deltaY
			if (e.deltaMode === 1) dy *= 16
			else if (e.deltaMode === 2) dy *= 800
			const current = pending?.z ?? viewZoom
			queueZoom(current * Math.exp(-dy * 0.01), e.clientX, e.clientY)
		}

		const onGestureStart = (e: Event) => {
			e.preventDefault()
			gestureBase = pending?.z ?? viewZoom
		}

		const onGestureChange = (e: Event) => {
			e.preventDefault()
			const ge = e as Event & {
				scale?: number
				clientX?: number
				clientY?: number
			}
			queueZoom(gestureBase * (ge.scale ?? 1), ge.clientX ?? 0, ge.clientY ?? 0)
		}

		const onGestureEnd = (e: Event) => {
			e.preventDefault()
		}

		el.addEventListener('wheel', onWheel, { passive: false })
		el.addEventListener('gesturestart', onGestureStart, { passive: false })
		el.addEventListener('gesturechange', onGestureChange, { passive: false })
		el.addEventListener('gestureend', onGestureEnd, { passive: false })
		return () => {
			if (raf) cancelAnimationFrame(raf)
			el.removeEventListener('wheel', onWheel)
			el.removeEventListener('gesturestart', onGestureStart)
			el.removeEventListener('gesturechange', onGestureChange)
			el.removeEventListener('gestureend', onGestureEnd)
		}
	})

	function setSelection(ids: string[]) {
		selectedTableIds = [...new Set(ids)]
		selectedEdgeId = null
	}

	function selectOnly(tableId: string) {
		setSelection([tableId])
	}

	function toggleSelect(tableId: string) {
		if (selectedTableIds.includes(tableId)) {
			selectedTableIds = selectedTableIds.filter((id) => id !== tableId)
		} else {
			selectedTableIds = [...selectedTableIds, tableId]
		}
		selectedEdgeId = null
	}

	function selectAllTables() {
		if (!diagram) return
		setSelection(diagram.nodes.map((n) => n.id))
	}

	function onColumnClick(tableId: string, _columnId: string, e: MouseEvent) {
		e.stopPropagation()
		e.preventDefault()
		ctxMenu = null
		if (e.ctrlKey || e.metaKey) toggleSelect(tableId)
		else selectOnly(tableId)
	}

	function onCanvasClick(e: MouseEvent) {
		const t = e.target as HTMLElement
		if (
			t.closest('[data-er-table]') ||
			t.closest('[data-er-column]') ||
			t.closest('[data-er-edge-hit]') ||
			t.closest('[data-ctx-menu]')
		) {
			return
		}
		ctxMenu = null
		selectedTableIds = []
		selectedEdgeId = null
	}

	function onCanvasPointerMove(e: PointerEvent) {
		if (!dragTableId || !diagram || !canvasEl) return
		const pt = canvasPoint(e)
		if (!pt) return
		scheduleTableDrag(pt)
	}

	function previewFromPointer(
		pt: ErPoint,
	): Record<string, { x: number; y: number }> | null {
		if (!dragTableId || !diagram) return null
		const x = Math.max(0, pt.x - dragOffX)
		const y = Math.max(0, pt.y - dragOffY)
		if (
			multiDragOrigins &&
			selectedTableIds.includes(dragTableId) &&
			selectedTableIds.length > 1
		) {
			const origin = multiDragOrigins[dragTableId]
			if (!origin) return null
			const dx = x - origin.x
			const dy = y - origin.y
			const next: Record<string, { x: number; y: number }> = {}
			for (const n of diagram.nodes) {
				if (!selectedTableIds.includes(n.id)) continue
				const o = multiDragOrigins[n.id]
				if (!o) continue
				next[n.id] = {
					x: Math.max(0, o.x + dx),
					y: Math.max(0, o.y + dy),
				}
			}
			return next
		}
		return { [dragTableId]: { x, y } }
	}

	function scheduleTableDrag(pt: ErPoint) {
		const next = previewFromPointer(pt)
		if (!next) return
		const moved = Object.entries(next).some(([id, p]) => {
			const n = diagram?.nodes.find((node) => node.id === id)
			return !n || n.x !== p.x || n.y !== p.y
		})
		if (moved) didDrag = true
		pendingDrag = next
		if (dragRaf) return
		dragRaf = requestAnimationFrame(() => {
			dragRaf = 0
			if (pendingDrag) {
				dragPreview = pendingDrag
				pendingDrag = null
			}
		})
	}

	function commitTableDrag() {
		if (dragRaf) {
			cancelAnimationFrame(dragRaf)
			dragRaf = 0
		}
		const preview = pendingDrag ?? dragPreview
		pendingDrag = null
		if (preview && diagram && didDrag) {
			diagram = {
				...diagram,
				nodes: diagram.nodes.map((n) => {
					const p = preview[n.id]
					if (!p) return n
					return {
						...n,
						x: snapToGrid(p.x),
						y: snapToGrid(p.y),
					}
				}),
			}
			invalidateDbml()
		}
		dragPreview = null
	}

	function onWindowPointerUp() {
		if (dragTableId) {
			commitTableDrag()
			dragTableId = null
			multiDragOrigins = null
		}
	}

	function onTablePointerDown(tableId: string, e: PointerEvent) {
		if (!diagram || !canvasEl) return
		const node = diagram.nodes.find((n) => n.id === tableId)
		if (!node) return

		if (e.ctrlKey || e.metaKey) {
			toggleSelect(tableId)
		} else if (!selectedTableIds.includes(tableId)) {
			selectOnly(tableId)
		}

		const origin = canvasPoint(e)
		if (!origin) return
		dragTableId = tableId
		didDrag = false
		dragOffX = origin.x - node.x
		dragOffY = origin.y - node.y
		selectedEdgeId = null
		ctxMenu = null

		if (selectedTableIds.includes(tableId) && selectedTableIds.length > 1) {
			multiDragOrigins = {}
			for (const n of diagram.nodes) {
				if (selectedTableIds.includes(n.id)) {
					multiDragOrigins[n.id] = { x: n.x, y: n.y }
				}
			}
		} else {
			multiDragOrigins = null
		}

		;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
	}

	function onTablePointerUp(e: PointerEvent) {
		if (dragTableId) {
			commitTableDrag()
			try {
				;(e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId)
			} catch {
				/* ignore */
			}
		}
		dragTableId = null
		multiDragOrigins = null
	}

	function onTableContextMenu(_tableId: string, e: MouseEvent) {
		e.preventDefault()
		e.stopPropagation()
		ctxMenu = { clientX: e.clientX, clientY: e.clientY }
	}

	function onCanvasContextMenu(e: MouseEvent) {
		const t = e.target as HTMLElement
		if (t.closest('[data-er-table]') || t.closest('[data-ctx-menu]')) return
		e.preventDefault()
		ctxMenu = { clientX: e.clientX, clientY: e.clientY }
	}

	function onCtxPick(id: string) {
		ctxMenu = null
		if (id === 'download-png') void downloadPng()
	}

	function onKeyDown(e: KeyboardEvent) {
		if (isEditableTarget(e.target)) return
		const mod = e.ctrlKey || e.metaKey
		const key = e.key.toLowerCase()

		if (mod && e.shiftKey && key === 'l') {
			e.preventDefault()
			autoLayout()
			return
		}
		if (mod && e.shiftKey && key === 'e') {
			e.preventDefault()
			void downloadPng()
			return
		}
		if (mod && (key === '=' || key === '+' || e.code === 'NumpadAdd')) {
			e.preventDefault()
			zoomIn()
			return
		}
		if (
			mod &&
			(key === '-' || e.code === 'Minus' || e.code === 'NumpadSubtract')
		) {
			e.preventDefault()
			zoomOut()
			return
		}
		if (mod && key === '0') {
			e.preventDefault()
			resetZoom()
			return
		}
		if (mod && key === 'a') {
			e.preventDefault()
			selectAllTables()
			return
		}
		if (e.key === 'Escape') {
			ctxMenu = null
			selectedTableIds = []
			selectedEdgeId = null
		}
	}

	function selectEdge(edgeId: string, e?: Event) {
		e?.stopPropagation()
		e?.preventDefault()
		selectedEdgeId = edgeId
		selectedTableIds = []
		ctxMenu = null
	}

	function toggleGrid() {
		showGrid = !showGrid
	}

	async function downloadPng() {
		if (exportingPng) return
		const live = displayDiagram
		const root = exportRoot
		if (!live || live.nodes.length === 0 || !root) {
			showToast('warning', 'Nothing to export.')
			return
		}
		exportingPng = true
		try {
			const bytes = await captureErDiagramPng({
				root,
				nodes: live.nodes,
				tableW: TABLE_W,
				headerH: HEADER_H,
				rowH: ROW_H,
			})
			const res = await exportPng(bytes, diagramName || 'er-diagram')
			if (res.ok) {
				showToast('success', 'PNG downloaded.')
			} else if (!res.cancelled) {
				showToast('danger', res.error)
			}
		} catch (e) {
			showToast('danger', e instanceof Error ? e.message : String(e))
		} finally {
			exportingPng = false
		}
	}

	function toggleDbml() {
		showDbml = !showDbml
		if (showDbml) queueDbml()
	}

	function autoLayout() {
		if (!diagram) return
		diagram = {
			...diagram,
			nodes: layoutErNodes(diagram.nodes, diagram.edges, { scale: uiScale }),
		}
		invalidateDbml()
		requestCenterInCanvas()
	}

	async function copyDbml() {
		const text = dbmlText || diagramToDbml(diagram, diagramName)
		try {
			await navigator.clipboard.writeText(text)
			dbmlCopied = true
			showToast('success', 'DBML copied.')
			setTimeout(() => {
				dbmlCopied = false
			}, 1600)
		} catch {
			showToast('danger', 'Could not copy DBML — select the text manually.')
		}
	}
</script>

<svelte:window
	onpointerup={onWindowPointerUp}
	onpointercancel={onWindowPointerUp}
	onkeydown={onKeyDown}
/>

<div class="flex h-full min-h-0 flex-col">
	<div class="flex min-h-0 flex-1">
		<div class="flex min-h-0 min-w-0 flex-1 flex-col">
			{#if diagram}
				<ErCanvas
					{diagram}
					{geom}
					{edgeSpines}
					bind:canvasEl
					bind:exportRoot
					canvasW={canvasBounds.width}
					canvasH={canvasBounds.height}
					{viewZoom}
					{dragPreview}
					{showGrid}
					{entityWord}
					{selectedTableIds}
					{selectedEdgeId}
					{focusedLink}
					{fkColumnIds}
					{edgesDrawOrder}
					{dragTableId}
					oncanvasclick={onCanvasClick}
					onpointermove={onCanvasPointerMove}
					oncanvascontextmenu={onCanvasContextMenu}
					onselectedge={selectEdge}
					ontablepointerdown={onTablePointerDown}
					ontablepointerup={onTablePointerUp}
					ontablecontextmenu={onTableContextMenu}
					oncolumnclick={onColumnClick}
				/>
			{:else}
				<div class="relative min-h-0 min-w-0 flex-1"></div>
			{/if}
		</div>

		{#if showDbml}
			<ErDbmlSnapshot
				text={dbmlText}
				busy={dbmlBusy}
				copied={dbmlCopied}
				oncopy={() => void copyDbml()}
				onclose={() => {
					showDbml = false
				}}
			/>
		{/if}
	</div>

	<ErStatusBar
		{entityWord}
		hasDiagram={!!diagram}
		nodeCount={diagram?.nodes.length ?? 0}
		edgeCount={diagram?.edges.length ?? 0}
		{selectionCount}
		{hasNodes}
		{showGrid}
		{showDbml}
		{canZoomOut}
		{canZoomIn}
		{zoomLabel}
		{refreshing}
		{exportingPng}
		onAutoLayout={autoLayout}
		onDownloadPng={() => void downloadPng()}
		onRefresh={onrefresh}
		onZoomOut={zoomOut}
		onZoomIn={zoomIn}
		onResetZoom={resetZoom}
		onToggleGrid={toggleGrid}
		onToggleDbml={toggleDbml}
	/>
</div>

{#if ctxMenu}
	<ContextMenu
		x={ctxMenu.clientX}
		y={ctxMenu.clientY}
		items={ctxMenuItems}
		ariaLabel="Diagram actions"
		onpick={onCtxPick}
		onclose={() => (ctxMenu = null)}
	/>
{/if}
