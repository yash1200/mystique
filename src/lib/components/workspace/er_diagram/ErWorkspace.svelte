<script lang="ts">
	/**
	 * ER canvas orchestrator — live (read-only chrome) or designer.
	 * Geometry: `erGeometry`. Layout: `erLayout`. Chrome: sibling panes.
	 */
	import { ContextMenu } from '$lib/components/core'
	import type {
		DataFamily,
		ErColumn,
		ErDiagramState,
		ErEdge,
		ErEdgeKind,
		ErNode,
		ErPoint,
	} from '$lib/types/er'
	import {
		ER_GRID,
		FAMILY_META,
		columnTypesForFamily,
		defaultColumnType,
		emptyDiagram,
		snapToGrid,
	} from '$lib/types/er'
	import { cn } from '$lib/utils/cn'
	import { currentUiScale, onUiScaleChange } from '$lib/utils/uiScale'
	import { onMount, tick } from 'svelte'
	import { dbmlToDiagram, diagramToDbml } from './dbml'
	import ErCanvas from './ErCanvas.svelte'
	import ErDbmlEditor from './ErDbmlEditor.svelte'
	import ErDbmlSnapshot from './ErDbmlSnapshot.svelte'
	import ErObjectRail from './ErObjectRail.svelte'
	import ErStatusBar from './ErStatusBar.svelte'
	import ErToolsPanel from './ErToolsPanel.svelte'
	import {
		ER_HEADER_H_BASE,
		ER_MAX_HISTORY,
		ER_ROW_H_BASE,
		ER_TABLE_W_BASE,
		ER_ZOOM_MAX,
		ER_ZOOM_MIN,
		clampErZoom,
		cloneDiagram,
		erEdgesDrawOrder,
		erFkColumnIds,
		erFocusedLink,
		filterErNavNodes,
		isEditableTarget,
		nearestErZoomStop,
		uid,
	} from './erConstants'
	import { erCtxMenuItems, type ErCtxMenu } from './erContextMenu'
	import { createErGeometry } from './erGeometry'
	import { erContentBounds, layoutErNodes } from './erLayout'

	interface Props {
		family: DataFamily
		/**
		 * Seed canvas with a pre-built diagram (live schema / model).
		 * When set, skips the sample emptyDiagram for this family.
		 */
		initialDiagram?: ErDiagramState | null
		initialName?: string
		/** Workspace embed: live schema (read-only chrome). */
		embedded?: boolean
		/** Live schema: reload tables / FKs from the database. */
		onrefresh?: () => void
		refreshing?: boolean
	}

	let {
		family,
		initialDiagram = null,
		initialName = '',
		embedded = true,
		onrefresh,
		refreshing = false,
	}: Props = $props()

	let uiScale = $state(typeof document !== 'undefined' ? currentUiScale() : 1)
	const TABLE_W = $derived(Math.round(ER_TABLE_W_BASE * uiScale))
	const HEADER_H = $derived(Math.round(ER_HEADER_H_BASE * uiScale))
	const ROW_H = $derived(Math.round(ER_ROW_H_BASE * uiScale))
	const PASTE_OFFSET = $derived(Math.round(ER_GRID * 2 * uiScale))
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
	let diagramName = $state('Untitled diagram')
	/** Multi-select table ids (order = selection order; last is primary for editing). */
	let selectedTableIds = $state<string[]>([])
	let selectedEdgeId = $state<string | null>(null)
	let toolMsg = $state<string | null>(null)
	let dirty = $state(false)
	let showGrid = $state(true)
	/**
	 * Right-hand DBML snapshot panel.
	 * Generated lazily — never from a $derived on the live diagram,
	 * or every drag would rebuild a huge string and block paint.
	 */
	let showDbml = $state(false)
	let dbmlCopied = $state(false)
	let dbmlText = $state('')
	let dbmlBusy = $state(false)
	let dbmlStale = $state(true)
	let dbmlGen = 0
	/**
	 * Left-hand DBML source editor — user writes text → Apply builds the canvas.
	 * When true, the right DBML panel is forced closed.
	 */
	let dbmlSourceMode = $state(false)
	let dbmlEditorText = $state('')
	let dbmlParseError = $state<string | null>(null)
	let tableQuery = $state('')
	let tableSearchWrap: HTMLDivElement | null = $state(null)
	/** Canvas view zoom (independent of app UI scale). */
	let viewZoom = $state(1)

	/** Undo / redo stacks (diagram snapshots taken before each change). */
	let undoStack = $state<ErDiagramState[]>([])
	let redoStack = $state<ErDiagramState[]>([])
	/** When true, mutations must not push new history (undo/redo apply). */
	let historyLocked = false
	/** Snapshot once per continuous drag (tables / waypoints). */
	let dragHistoryPushed = false
	/** Snapshot once per rename typing session. */
	let renameHistoryPushed = false

	let arrowTool = $state<'normal' | 'broken' | null>(null)
	let linkFrom = $state<{ tableId: string; columnId: string } | null>(null)
	let draftWaypoints = $state<ErPoint[]>([])
	let cursorPt = $state<ErPoint | null>(null)

	// table drag (supports multi-drag)
	let dragTableId = $state<string | null>(null)
	let dragOffX = 0
	let dragOffY = 0
	let didDrag = false
	let multiDragOrigins: Record<string, { x: number; y: number }> | null = null
	let canvasEl: HTMLDivElement | null = $state(null)

	let dragWaypoint = $state<{ edgeId: string; index: number } | null>(null)

	/** Context menu: table or empty canvas */
	let ctxMenu = $state<ErCtxMenu | null>(null)

	/** Internal clipboard of tables only (no relationships). */
	let tableClipboard = $state<ErNode[] | null>(null)

	let showAddCol = $state(false)
	let newColName = $state('')
	let newColType = $state(defaultColumnType('sql'))
	let newColPk = $state(false)

	const meta = $derived(FAMILY_META[family])
	/** Live DB diagram from workspace — view/rearrange only, no design tools. */
	const readOnly = $derived(embedded)
	const typeOptions = $derived(
		columnTypesForFamily(family).map((t) => ({ value: t, label: t })),
	)
	/** Primary table for rename / columns UI when exactly one selected. */
	const primaryTableId = $derived(
		selectedTableIds.length === 1 ? selectedTableIds[0]! : null,
	)
	const selectionCount = $derived(selectedTableIds.length)
	const canPasteTables = $derived(!!tableClipboard?.length)
	const ctxMenuItems = $derived(
		erCtxMenuItems({
			ctx: ctxMenu,
			selectionCount,
			canPaste: canPasteTables,
		}),
	)
	const canUndo = $derived(undoStack.length > 0)
	const canRedo = $derived(redoStack.length > 0)
	/** Right panel only when snapshot view is on AND source editor is off. */
	const showRightDbml = $derived(showDbml && !dbmlSourceMode)
	const entityWord = $derived(meta.nodeLabel.toLowerCase())
	const navNodes = $derived(
		diagram ? filterErNavNodes(diagram.nodes, tableQuery) : [],
	)
	const fkColumnIds = $derived(erFkColumnIds(diagram?.edges ?? []))
	/** When a link is selected, only its two tables stay bright. */
	const focusedLink = $derived(erFocusedLink(diagram, selectedEdgeId))
	const edgesDrawOrder = $derived(
		erEdgesDrawOrder(diagram?.edges ?? [], selectedEdgeId),
	)
	const edgeSpines = $derived.by(() =>
		diagram ? geom.assignEdgeSpines(diagram) : new Map(),
	)
	const canvasBounds = $derived.by(() =>
		erContentBounds(diagram?.nodes ?? [], uiScale),
	)
	const worldW = $derived(
		Math.max(1, Math.round(canvasBounds.width * viewZoom)),
	)
	const worldH = $derived(
		Math.max(1, Math.round(canvasBounds.height * viewZoom)),
	)
	const zoomLabel = $derived(`${Math.round(viewZoom * 100)}%`)
	const canZoomOut = $derived(viewZoom > ER_ZOOM_MIN + 0.001)
	const canZoomIn = $derived(viewZoom < ER_ZOOM_MAX - 0.001)
	const hasNodes = $derived(!!diagram && diagram.nodes.length > 0)

	/**
	 * Reset canvas when family or live diagram identity changes.
	 * Live diagrams pass a new `initialDiagram` reference when the DB reloads.
	 */
	let lastInitKey = ''

	$effect(() => {
		const f = family
		const live = initialDiagram
		const nameHint = initialName?.trim() || ''
		// Key: family + live node/edge counts + name (cheap identity for reloads)
		const liveKey = live
			? `live:${live.nodes.length}:${live.edges.length}:${nameHint}`
			: 'empty'
		const key = `${f}::${liveKey}`
		if (lastInitKey === key) return
		lastInitKey = key

		if (live && live.nodes) {
			// JSON clone avoids $state proxy issues on seeded live diagrams
			const cloned = cloneDiagram({
				family: live.family || f,
				nodes: live.nodes,
				edges: live.edges ?? [],
			})
			diagram = {
				...cloned,
				nodes: layoutErNodes(cloned.nodes, cloned.edges, { scale: uiScale }),
			}
			diagramName =
				nameHint ||
				(embedded
					? 'Live schema diagram'
					: `Untitled ${FAMILY_META[f].short} diagram`)
			toolMsg = null
		} else {
			diagram = emptyDiagram(f)
			diagramName = nameHint || `Untitled ${FAMILY_META[f].short} diagram`
			toolMsg =
				'DBML editor (left) builds the diagram from text · Show DBML for a copyable export.'
		}
		selectedTableIds = []
		selectedEdgeId = null
		arrowTool = null
		linkFrom = null
		draftWaypoints = []
		cursorPt = null
		showAddCol = false
		dirty = false
		ctxMenu = null
		dragWaypoint = null
		tableClipboard = null
		newColType = defaultColumnType(f)
		dbmlSourceMode = false
		dbmlEditorText = ''
		dbmlParseError = null
		showDbml = false
		dbmlText = ''
		dbmlBusy = false
		dbmlStale = true
		dbmlGen += 1
		tableQuery = ''
		viewZoom = 1
		clearHistory()
		renameHistoryPushed = false
	})

	function clearHistory() {
		undoStack = []
		redoStack = []
		historyLocked = false
		dragHistoryPushed = false
	}

	/** Call immediately before mutating `diagram` so the prior state is undoable. */
	function snapshotBeforeChange() {
		if (!diagram || historyLocked) return
		try {
			undoStack = [...undoStack, cloneDiagram(diagram)].slice(-ER_MAX_HISTORY)
			redoStack = []
		} catch (err) {
			console.warn('ER history snapshot failed', err)
		}
	}

	function markDirty(opts?: { structure?: boolean }) {
		// Live workspace ER is a view of the catalog — positions are not persisted.
		if (!readOnly) dirty = true
		if (opts?.structure !== false) invalidateDbml()
	}

	function invalidateDbml() {
		dbmlStale = true
		if (showRightDbml) queueDbml()
	}

	/** Yield a frame so opening the panel never blocks chrome paint. */
	function queueDbml() {
		if (!showDbml || dbmlSourceMode) return
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

	function pruneSelectionToDiagram() {
		if (!diagram) {
			selectedTableIds = []
			selectedEdgeId = null
			return
		}
		const nodeIds = new Set(diagram.nodes.map((n) => n.id))
		selectedTableIds = selectedTableIds.filter((id) => nodeIds.has(id))
		if (selectedEdgeId && !diagram.edges.some((e) => e.id === selectedEdgeId)) {
			selectedEdgeId = null
		}
	}

	function undo() {
		if (!diagram || undoStack.length === 0) {
			toolMsg = 'Nothing to undo.'
			return
		}
		historyLocked = true
		redoStack = [cloneDiagram(diagram), ...redoStack].slice(0, ER_MAX_HISTORY)
		diagram = undoStack[undoStack.length - 1]!
		undoStack = undoStack.slice(0, -1)
		linkFrom = null
		draftWaypoints = []
		cursorPt = null
		showAddCol = false
		ctxMenu = null
		dragWaypoint = null
		pruneSelectionToDiagram()
		markDirty()
		historyLocked = false
		toolMsg = 'Undid last change.'
	}

	function redo() {
		if (!diagram || redoStack.length === 0) {
			toolMsg = 'Nothing to redo.'
			return
		}
		historyLocked = true
		undoStack = [...undoStack, cloneDiagram(diagram)].slice(-ER_MAX_HISTORY)
		diagram = redoStack[0]!
		redoStack = redoStack.slice(1)
		linkFrom = null
		draftWaypoints = []
		cursorPt = null
		showAddCol = false
		ctxMenu = null
		dragWaypoint = null
		pruneSelectionToDiagram()
		markDirty()
		historyLocked = false
		toolMsg = 'Redid last change.'
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

	async function setViewZoom(
		next: number,
		origin?: { clientX: number; clientY: number },
	) {
		const z = clampErZoom(next)
		if (Math.abs(z - viewZoom) < 0.001) return
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
		viewZoom = z
		await tick()
		if (!el) return
		el.scrollLeft = worldX * z - ox
		el.scrollTop = worldY * z - oy
	}

	function zoomIn() {
		const next = nearestErZoomStop(viewZoom, 1)
		void setViewZoom(next)
		toolMsg = `Zoom ${Math.round(next * 100)}%.`
	}

	function zoomOut() {
		const next = nearestErZoomStop(viewZoom, -1)
		void setViewZoom(next)
		toolMsg = `Zoom ${Math.round(next * 100)}%.`
	}

	function resetZoom() {
		void setViewZoom(1)
		toolMsg = 'Zoom 100%.'
	}

	$effect(() => {
		const el = canvasEl
		if (!el) return
		const onWheel = (e: WheelEvent) => {
			if (!(e.ctrlKey || e.metaKey)) return
			e.preventDefault()
			const next =
				e.deltaY > 0
					? nearestErZoomStop(viewZoom, -1)
					: nearestErZoomStop(viewZoom, 1)
			void setViewZoom(next, { clientX: e.clientX, clientY: e.clientY })
		}
		el.addEventListener('wheel', onWheel, { passive: false })
		return () => el.removeEventListener('wheel', onWheel)
	})

	function setSelection(ids: string[]) {
		selectedTableIds = [...new Set(ids)]
		selectedEdgeId = null
		if (selectedTableIds.length !== 1) showAddCol = false
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
		showAddCol = false
	}

	function selectAllTables() {
		if (!diagram) return
		setSelection(diagram.nodes.map((n) => n.id))
		toolMsg = `Selected ${diagram.nodes.length} table(s). Right-click → Delete, or Ctrl+C to copy.`
	}

	function setArrowTool(kind: 'normal' | 'broken' | null) {
		if (readOnly) return
		arrowTool = kind
		linkFrom = null
		draftWaypoints = []
		cursorPt = null
		selectedEdgeId = null
		ctxMenu = null
		dragWaypoint = null
		if (kind === 'normal') {
			toolMsg =
				'1) Click a column on table A  ·  2) Optional: click canvas to bend the path  ·  3) Click a column on table B'
		} else if (kind === 'broken') {
			toolMsg =
				'Broken (dashed) link: click source column → optional path points → target column'
		} else {
			toolMsg =
				'Move mode: drag tables · Ctrl+click multi-select · Ctrl+A all · Ctrl+C/V copy/paste · Right-click Delete.'
		}
	}

	function cancelLink() {
		linkFrom = null
		draftWaypoints = []
		cursorPt = null
		toolMsg = arrowTool
			? 'Link cancelled. Click a source column to start again.'
			: 'Move mode.'
	}

	function addTable() {
		if (readOnly) return
		if (!diagram) {
			diagram = emptyDiagram(family)
		}
		if (arrowTool) {
			linkFrom = null
			draftWaypoints = []
			cursorPt = null
		}
		ctxMenu = null
		const current = diagram
		const n = current.nodes.length
		const kind: ErNode['kind'] =
			family === 'sql'
				? 'table'
				: family === 'nosql'
					? 'collection'
					: 'key_pattern'
		const node: ErNode = {
			id: uid('n'),
			kind,
			name:
				family === 'sql'
					? `table_${n + 1}`
					: family === 'nosql'
						? `collection_${n + 1}`
						: `key:pattern_${n + 1}`,
			columns: [
				{
					id: uid('c'),
					name: family === 'sql' ? 'id' : family === 'nosql' ? '_id' : 'type',
					dataType:
						family === 'sql'
							? 'UUID'
							: family === 'nosql'
								? 'ObjectId'
								: 'string',
					pk: family === 'sql' || family === 'nosql',
				},
			],
			x: snapToGrid(ER_GRID * 4 + (n % 4) * ER_GRID * 2),
			y: snapToGrid(ER_GRID * 4 + (n % 4) * ER_GRID * 2),
		}
		snapshotBeforeChange()
		diagram = { ...current, nodes: [...current.nodes, node] }
		selectOnly(node.id)
		showAddCol = false
		toolMsg = `Added “${node.name}”.`
		markDirty()
	}

	function onColumnClick(tableId: string, columnId: string, e: MouseEvent) {
		e.stopPropagation()
		e.preventDefault()
		ctxMenu = null

		if (readOnly) {
			if (e.ctrlKey || e.metaKey) toggleSelect(tableId)
			else selectOnly(tableId)
			return
		}

		if (!arrowTool) {
			if (e.ctrlKey || e.metaKey) toggleSelect(tableId)
			else selectOnly(tableId)
			showAddCol = false
			return
		}

		if (!linkFrom) {
			linkFrom = { tableId, columnId }
			draftWaypoints = []
			selectOnly(tableId)
			const node = diagram?.nodes.find((n) => n.id === tableId)
			const col = node?.columns.find((c) => c.id === columnId)
			toolMsg = `From ${node?.name ?? '?'}.${col?.name ?? '?'}. Click target column (or same table for self-loop). Path is always 90° grid corridors — click empty grid for square bends.`
			return
		}

		if (linkFrom.tableId === tableId && linkFrom.columnId === columnId) {
			toolMsg =
				'Pick a different target column (self-loop needs two columns on this table).'
			return
		}

		completeLink(tableId, columnId)
	}

	function completeLink(toTableId: string, toColumnId: string) {
		if (!diagram || !linkFrom || !arrowTool) return

		if (linkFrom.tableId === toTableId && linkFrom.columnId === toColumnId) {
			toolMsg = 'Cannot link a column to itself — choose another column.'
			return
		}

		const exists = diagram.edges.some(
			(ed) =>
				ed.from === linkFrom!.tableId &&
				ed.fromColumnId === linkFrom!.columnId &&
				ed.to === toTableId &&
				ed.toColumnId === toColumnId,
		)
		if (exists) {
			toolMsg = 'That column link already exists.'
			linkFrom = null
			draftWaypoints = []
			cursorPt = null
			arrowTool = null
			return
		}

		const fromNode = diagram.nodes.find((n) => n.id === linkFrom!.tableId)
		const toNode = diagram.nodes.find((n) => n.id === toTableId)
		const fromCol = fromNode?.columns.find((c) => c.id === linkFrom!.columnId)
		const toCol = toNode?.columns.find((c) => c.id === toColumnId)
		const isSelfLoop = linkFrom.tableId === toTableId
		const label = `${fromNode?.name}.${fromCol?.name} → ${toNode?.name}.${toCol?.name}`
		const kind: ErEdgeKind = arrowTool === 'broken' ? 'broken' : 'normal'

		let wps = draftWaypoints.map((p) => ({
			x: snapToGrid(p.x),
			y: snapToGrid(p.y),
		}))

		const lane = diagram.edges.filter(
			(e) => e.from === linkFrom!.tableId && e.to === toTableId,
		).length

		if (wps.length === 0 && fromNode && toNode) {
			const { fromSide, toSide } = geom.pickLinkSides(fromNode, toNode, [])
			const start = geom.columnSideAnchor(fromNode, linkFrom.columnId, fromSide)
			const end = geom.columnSideAnchor(toNode, toColumnId, toSide)
			if (isSelfLoop) {
				wps = geom
					.autoSelfLoopWaypoints(fromNode, linkFrom.columnId, toColumnId)
					.map((p) => ({
						x: snapToGrid(
							p.x + (fromSide === 'right' ? lane : -lane) * ER_GRID,
						),
						y: p.y,
					}))
			} else {
				wps = geom.defaultManhattanWaypoints(start, end, fromSide, toSide)
			}
		}

		const edge: ErEdge = {
			id: uid('e'),
			from: linkFrom.tableId,
			to: toTableId,
			fromColumnId: linkFrom.columnId,
			toColumnId: toColumnId,
			kind,
			label,
			waypoints: wps,
		}

		snapshotBeforeChange()
		diagram = { ...diagram, edges: [...diagram.edges, edge] }
		toolMsg = isSelfLoop
			? `Self-loop: ${label}. 90° path. Arrow tool cleared.`
			: `Linked: ${label}. 90° grid path. Arrow tool cleared.`

		linkFrom = null
		draftWaypoints = []
		cursorPt = null
		arrowTool = null
		selectedEdgeId = null
		selectedTableIds = []
		markDirty()
	}

	function onCanvasClick(e: MouseEvent) {
		const t = e.target as HTMLElement
		if (
			t.closest('[data-er-table]') ||
			t.closest('[data-er-column]') ||
			t.closest('[data-er-edge-handle]') ||
			t.closest('[data-er-edge-hit]') ||
			t.closest('[data-ctx-menu]')
		) {
			return
		}

		ctxMenu = null

		if (arrowTool && linkFrom) {
			const pt = canvasPoint(e)
			if (!pt || !diagram) return
			const last = geom.linkDraftLast(
				diagram,
				linkFrom,
				draftWaypoints,
				cursorPt,
			)
			const snapped = last
				? geom.orthoGridSnapFrom(last, pt)
				: { x: snapToGrid(pt.x), y: snapToGrid(pt.y) }
			if (last && geom.samePoint(last, snapped)) return
			draftWaypoints = [...draftWaypoints, snapped]
			toolMsg = `Square bend ${draftWaypoints.length} (90°). Click more grid points or the target column.`
			return
		}

		selectedTableIds = []
		selectedEdgeId = null
		showAddCol = false
	}

	function onCanvasPointerMove(e: PointerEvent) {
		if (arrowTool && linkFrom) {
			cursorPt = canvasPoint(e)
		}

		if (dragWaypoint && diagram) {
			const pt = canvasPoint(e)
			if (!pt) return
			const { edgeId, index } = dragWaypoint
			if (!dragHistoryPushed) {
				snapshotBeforeChange()
				dragHistoryPushed = true
			}
			let nextIndex = index
			const nextEdges = diagram.edges.map((ed) => {
				if (ed.id !== edgeId) return ed
				const wps = [...(ed.waypoints ?? [])]
				if (index < 0 || index >= wps.length) return ed
				wps[index] = { x: snapToGrid(pt.x), y: snapToGrid(pt.y) }
				const normalized = geom.normalizeEdgeWaypoints(
					diagram!,
					edgeSpines,
					ed,
					wps,
				)
				nextIndex = Math.min(index, Math.max(0, normalized.length - 1))
				return { ...ed, waypoints: normalized }
			})
			diagram = { ...diagram, edges: nextEdges }
			if (nextIndex !== index) {
				dragWaypoint = { edgeId, index: nextIndex }
			}
			return
		}

		if (!dragTableId || !diagram || !canvasEl) return
		const pt = canvasPoint(e)
		if (!pt) return
		const x = snapToGrid(Math.max(0, pt.x - dragOffX))
		const y = snapToGrid(Math.max(0, pt.y - dragOffY))

		if (
			multiDragOrigins &&
			selectedTableIds.includes(dragTableId) &&
			selectedTableIds.length > 1
		) {
			const origin = multiDragOrigins[dragTableId]
			if (!origin) return
			const dx = x - origin.x
			const dy = y - origin.y
			if (dx !== 0 || dy !== 0) {
				if (!dragHistoryPushed) {
					snapshotBeforeChange()
					dragHistoryPushed = true
				}
				didDrag = true
			}
			diagram = {
				...diagram,
				nodes: diagram.nodes.map((n) => {
					if (!selectedTableIds.includes(n.id)) return n
					const o = multiDragOrigins![n.id]
					if (!o) return n
					return {
						...n,
						x: snapToGrid(Math.max(0, o.x + dx)),
						y: snapToGrid(Math.max(0, o.y + dy)),
					}
				}),
			}
			return
		}

		const cur = diagram.nodes.find((n) => n.id === dragTableId)
		if (cur && (cur.x !== x || cur.y !== y)) {
			if (!dragHistoryPushed) {
				snapshotBeforeChange()
				dragHistoryPushed = true
			}
			didDrag = true
		}
		diagram = {
			...diagram,
			nodes: diagram.nodes.map((n) =>
				n.id === dragTableId ? { ...n, x, y } : n,
			),
		}
	}

	function onWindowPointerUp() {
		if (dragWaypoint) {
			markDirty({ structure: false })
			dragWaypoint = null
		}
	}

	function onTablePointerDown(tableId: string, e: PointerEvent) {
		if (arrowTool) return
		if ((e.target as HTMLElement).closest('[data-no-drag]')) return
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
		dragHistoryPushed = false
		dragOffX = origin.x - node.x
		dragOffY = origin.y - node.y
		selectedEdgeId = null
		ctxMenu = null
		renameHistoryPushed = false

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
			if (didDrag) markDirty({ structure: false })
			try {
				;(e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId)
			} catch {
				/* ignore */
			}
		}
		dragTableId = null
		multiDragOrigins = null
	}

	function onTableContextMenu(tableId: string, e: MouseEvent) {
		if (readOnly) {
			e.preventDefault()
			return
		}
		e.preventDefault()
		e.stopPropagation()
		if (!selectedTableIds.includes(tableId)) {
			selectOnly(tableId)
		}
		selectedEdgeId = null
		ctxMenu = { clientX: e.clientX, clientY: e.clientY, kind: 'table', tableId }
	}

	function onCanvasContextMenu(e: MouseEvent) {
		if (readOnly) {
			e.preventDefault()
			return
		}
		const t = e.target as HTMLElement
		if (t.closest('[data-er-table]') || t.closest('[data-ctx-menu]')) return
		e.preventDefault()
		ctxMenu = { clientX: e.clientX, clientY: e.clientY, kind: 'canvas' }
	}

	function onCtxPick(id: string) {
		const ctx = ctxMenu
		ctxMenu = null
		if (!ctx) return
		if (id === 'select-all') selectAllTables()
		if (id === 'paste') pasteTables()
		if (id === 'copy') copySelectedTables()
		if (id === 'delete') {
			if (ctx.kind === 'canvas') {
				deleteTablesByIds([...selectedTableIds])
				return
			}
			const ids =
				selectedTableIds.length > 0
					? [...selectedTableIds]
					: ctx.tableId
						? [ctx.tableId]
						: []
			if (ids.length) deleteTablesByIds(ids)
			else deleteSelection()
		}
	}

	function deleteTablesByIds(ids: string[]) {
		if (!diagram || !ids.length) return
		const set = new Set(ids)
		const idList = ids.filter(Boolean)
		if (!idList.length) return
		snapshotBeforeChange()
		const nextNodes = diagram.nodes.filter((n) => !set.has(n.id))
		const nextEdges = diagram.edges.filter(
			(e) => !set.has(e.from) && !set.has(e.to),
		)
		diagram = {
			...diagram,
			nodes: nextNodes,
			edges: nextEdges,
		}
		selectedTableIds = selectedTableIds.filter((id) => !set.has(id))
		selectedEdgeId = null
		showAddCol = false
		ctxMenu = null
		toolMsg =
			idList.length === 1
				? 'Table deleted.'
				: `Deleted ${idList.length} tables.`
		markDirty()
	}

	function deleteSelection() {
		if (readOnly) return
		const tableIds = [...selectedTableIds]
		const edgeId = selectedEdgeId
		if (tableIds.length > 0) {
			deleteTablesByIds(tableIds)
			return
		}
		if (edgeId) {
			deleteEdge(edgeId)
		}
	}

	function copySelectedTables() {
		if (!diagram || !selectedTableIds.length) {
			toolMsg = 'Select one or more tables to copy (Ctrl+C).'
			return
		}
		const nodes = diagram.nodes
			.filter((n) => selectedTableIds.includes(n.id))
			.map((n) => ({
				...n,
				columns: n.columns.map((c) => ({ ...c })),
			}))
		tableClipboard = nodes
		toolMsg = `Copied ${nodes.length} table(s) without relationships. Ctrl+V to paste.`
	}

	function pasteTables() {
		if (!diagram || !tableClipboard?.length) {
			toolMsg = 'Clipboard empty — copy tables with Ctrl+C first.'
			return
		}
		const newIds: string[] = []
		const pasted: ErNode[] = tableClipboard.map((src) => {
			const columns = src.columns.map((c) => {
				const nid = uid('c')
				return { ...c, id: nid }
			})
			const id = uid('n')
			newIds.push(id)
			return {
				...src,
				id,
				name: `${src.name}_copy`,
				columns,
				x: snapToGrid(src.x + PASTE_OFFSET),
				y: snapToGrid(src.y + PASTE_OFFSET),
			}
		})
		tableClipboard = tableClipboard.map((n) => ({
			...n,
			columns: n.columns.map((c) => ({ ...c })),
			x: n.x + PASTE_OFFSET,
			y: n.y + PASTE_OFFSET,
		}))
		snapshotBeforeChange()
		diagram = { ...diagram, nodes: [...diagram.nodes, ...pasted] }
		setSelection(newIds)
		toolMsg = `Pasted ${pasted.length} table(s) (no relationships).`
		markDirty()
	}

	function onKeyDown(e: KeyboardEvent) {
		const mod = e.ctrlKey || e.metaKey
		const key = e.key.toLowerCase()
		const isDeleteKey =
			e.key === 'Delete' ||
			e.key === 'Backspace' ||
			e.code === 'Delete' ||
			e.code === 'Backspace'

		if (isEditableTarget(e.target) && !isDeleteKey) {
			return
		}
		if (isEditableTarget(e.target) && isDeleteKey) {
			if (e.key === 'Backspace' || e.code === 'Backspace') return
		}

		if (e.key === '/' && !isEditableTarget(e.target)) {
			e.preventDefault()
			tableSearchWrap?.querySelector('input')?.focus()
			return
		}
		if (mod && e.shiftKey && key === 'l') {
			e.preventDefault()
			autoLayout()
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

		if (readOnly) {
			if (mod && key === 'a') {
				e.preventDefault()
				selectAllTables()
			}
			if (e.key === 'Escape') {
				ctxMenu = null
				selectedTableIds = []
				selectedEdgeId = null
				tableQuery = ''
			}
			return
		}

		if (mod && key === 'z' && !e.shiftKey) {
			e.preventDefault()
			undo()
			return
		}
		if (mod && (key === 'y' || (key === 'z' && e.shiftKey))) {
			e.preventDefault()
			redo()
			return
		}
		if (mod && key === 'a') {
			e.preventDefault()
			if (!arrowTool) selectAllTables()
			return
		}
		if (mod && key === 'c') {
			if (isEditableTarget(e.target)) return
			e.preventDefault()
			if (!arrowTool) copySelectedTables()
			return
		}
		if (mod && key === 'v') {
			if (isEditableTarget(e.target)) return
			e.preventDefault()
			if (!arrowTool) pasteTables()
			return
		}
		if (isDeleteKey) {
			if (selectedTableIds.length || selectedEdgeId) {
				e.preventDefault()
				e.stopPropagation()
				deleteSelection()
			}
			return
		}
		if (e.key === 'Escape') {
			ctxMenu = null
			if (linkFrom) cancelLink()
			else {
				selectedTableIds = []
				selectedEdgeId = null
			}
		}
	}

	function renameTable(name: string) {
		if (!diagram || !primaryTableId) return
		if (!renameHistoryPushed) {
			snapshotBeforeChange()
			renameHistoryPushed = true
		}
		diagram = {
			...diagram,
			nodes: diagram.nodes.map((n) =>
				n.id === primaryTableId ? { ...n, name: name || n.name } : n,
			),
		}
		markDirty()
	}

	function updateColumnType(tableId: string, colId: string, dataType: string) {
		if (!diagram) return
		snapshotBeforeChange()
		diagram = {
			...diagram,
			nodes: diagram.nodes.map((n) =>
				n.id === tableId
					? {
							...n,
							columns: n.columns.map((c) =>
								c.id === colId ? { ...c, dataType } : c,
							),
						}
					: n,
			),
		}
		markDirty()
	}

	function addColumn() {
		if (!diagram || !primaryTableId) return
		const name = newColName.trim()
		if (!name) {
			toolMsg = 'Column name is required.'
			return
		}
		const col: ErColumn = {
			id: uid('c'),
			name,
			dataType: newColType || defaultColumnType(family),
			pk: newColPk,
		}
		snapshotBeforeChange()
		diagram = {
			...diagram,
			nodes: diagram.nodes.map((n) => {
				if (n.id !== primaryTableId) return n
				let columns = [...n.columns, col]
				if (col.pk) {
					columns = columns.map((c) =>
						c.id === col.id ? c : { ...c, pk: false },
					)
				}
				return { ...n, columns }
			}),
		}
		newColName = ''
		newColType = defaultColumnType(family)
		newColPk = false
		showAddCol = false
		markDirty()
	}

	function deleteColumn(colId: string) {
		if (!diagram || !primaryTableId) return
		snapshotBeforeChange()
		diagram = {
			...diagram,
			nodes: diagram.nodes.map((n) =>
				n.id === primaryTableId
					? { ...n, columns: n.columns.filter((c) => c.id !== colId) }
					: n,
			),
			edges: diagram.edges.filter(
				(e) => e.fromColumnId !== colId && e.toColumnId !== colId,
			),
		}
		markDirty()
	}

	function deleteEdge(edgeId: string) {
		if (!diagram) return
		snapshotBeforeChange()
		diagram = {
			...diagram,
			edges: diagram.edges.filter((e) => e.id !== edgeId),
		}
		if (selectedEdgeId === edgeId) selectedEdgeId = null
		toolMsg = 'Link deleted.'
		markDirty()
	}

	function edgeEndpointLabel(edge: ErEdge, which: 'from' | 'to'): string {
		if (!diagram) return '?'
		const node = diagram.nodes.find((n) => n.id === edge[which])
		const colId = which === 'from' ? edge.fromColumnId : edge.toColumnId
		const col = node?.columns.find((c) => c.id === colId)
		if (!node) return '?'
		return col ? `${node.name}.${col.name}` : node.name
	}

	function selectEdge(edgeId: string, e?: Event) {
		e?.stopPropagation()
		e?.preventDefault()
		selectedEdgeId = edgeId
		selectedTableIds = []
		showAddCol = false
		ctxMenu = null
		const edge = diagram?.edges.find((ed) => ed.id === edgeId)
		if (edge) {
			toolMsg = `${edgeEndpointLabel(edge, 'from')} → ${edgeEndpointLabel(edge, 'to')} · other tables dimmed. Click canvas to clear.`
		} else if (!arrowTool) {
			toolMsg =
				'Link selected. Drag handles to reshape. Delete key removes the link.'
		}
	}

	function onWaypointDown(edgeId: string, index: number, e: PointerEvent) {
		e.stopPropagation()
		e.preventDefault()
		selectedEdgeId = edgeId
		selectedTableIds = []
		dragHistoryPushed = false
		dragWaypoint = { edgeId, index }
		;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
	}

	function onWaypointUp(e: PointerEvent) {
		if (dragWaypoint) {
			markDirty({ structure: false })
			dragWaypoint = null
		}
		try {
			;(e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId)
		} catch {
			/* ignore */
		}
	}

	function onMidHandleDown(edgeId: string, insertAt: number, e: PointerEvent) {
		e.stopPropagation()
		e.preventDefault()
		if (!diagram) return
		const edge = diagram.edges.find((ed) => ed.id === edgeId)
		if (!edge) return
		const pts = geom.edgePoints(diagram, edgeSpines, edge)
		if (insertAt < 0 || insertAt >= pts.length - 1) return
		const a = pts[insertAt]!
		const b = pts[insertAt + 1]!
		const mid = {
			x: snapToGrid((a.x + b.x) / 2),
			y: snapToGrid((a.y + b.y) / 2),
		}
		const seeded = [
			...pts.slice(0, insertAt + 1),
			mid,
			...pts.slice(insertAt + 1),
		]
		const wps = geom.normalizeEdgeWaypoints(
			diagram,
			edgeSpines,
			edge,
			seeded.slice(1, -1),
		)
		const dragIndex = Math.min(insertAt, Math.max(0, wps.length - 1))
		snapshotBeforeChange()
		dragHistoryPushed = true
		diagram = {
			...diagram,
			edges: diagram.edges.map((ed) =>
				ed.id === edgeId ? { ...ed, waypoints: wps } : ed,
			),
		}
		selectedEdgeId = edgeId
		selectedTableIds = []
		dragWaypoint = { edgeId, index: dragIndex }
		;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
	}

	function removeWaypoint(edgeId: string, index: number, e: Event) {
		e.stopPropagation()
		e.preventDefault()
		if (!diagram) return
		snapshotBeforeChange()
		diagram = {
			...diagram,
			edges: diagram.edges.map((ed) => {
				if (ed.id !== edgeId) return ed
				const wps = [...(ed.waypoints ?? [])]
				wps.splice(index, 1)
				return {
					...ed,
					waypoints: geom.normalizeEdgeWaypoints(
						diagram!,
						edgeSpines,
						ed,
						wps,
					),
				}
			}),
		}
		toolMsg = 'Bend point removed (path stays 90°).'
		markDirty()
	}

	function straightenSelectedEdge() {
		if (!diagram || !selectedEdgeId) return
		snapshotBeforeChange()
		diagram = {
			...diagram,
			edges: diagram.edges.map((ed) => {
				if (ed.id !== selectedEdgeId) return ed
				const raw = geom.edgeAnchorsAndWaypoints(diagram!, {
					...ed,
					waypoints: [],
				})
				if (!raw) return { ...ed, waypoints: [] }
				const wps = geom.defaultManhattanWaypoints(
					raw.start,
					raw.end,
					raw.fromSide,
					raw.toSide,
					geom.edgeSpine(diagram!, edgeSpines, ed),
				)
				return { ...ed, waypoints: wps }
			}),
		}
		toolMsg = 'Path reset to default 90° corridor.'
		markDirty()
	}

	function toggleGrid() {
		showGrid = !showGrid
		toolMsg = showGrid ? 'Grid shown.' : 'Grid hidden.'
	}

	function toggleDbml() {
		if (dbmlSourceMode) {
			toolMsg = 'Close left DBML editor first to show the DBML panel.'
			return
		}
		showDbml = !showDbml
		if (showDbml) {
			toolMsg = 'Generating DBML…'
			queueDbml()
		} else {
			toolMsg = 'DBML panel hidden.'
		}
	}

	function autoLayout() {
		if (!diagram) return
		snapshotBeforeChange()
		diagram = {
			...diagram,
			nodes: layoutErNodes(diagram.nodes, diagram.edges, { scale: uiScale }),
		}
		if (!readOnly) markDirty()
		toolMsg =
			diagram.edges.length > 0
				? 'Auto-layout · parents left, children right.'
				: 'Auto-layout · packed grid.'
		requestAnimationFrame(() => {
			canvasEl?.scrollTo({ left: 0, top: 0 })
		})
	}

	function focusTable(id: string) {
		if (!diagram || !canvasEl) return
		const node = diagram.nodes.find((n) => n.id === id)
		if (!node) return
		selectOnly(id)
		const left = Math.max(0, node.x * viewZoom - ER_GRID * 2)
		const top = Math.max(0, node.y * viewZoom - ER_GRID * 2)
		canvasEl.scrollTo({ left, top, behavior: 'smooth' })
	}

	function openDbmlSourceEditor() {
		dbmlSourceMode = true
		showDbml = false
		dbmlParseError = null
		dbmlEditorText = diagramToDbml(diagram, diagramName)
		toolMsg = 'DBML editor open — edit text, then Apply to rebuild the diagram.'
	}

	function closeDbmlSourceEditor() {
		dbmlSourceMode = false
		dbmlParseError = null
		toolMsg =
			'DBML editor closed. Use tools to edit the canvas, or Show DBML for a snapshot.'
	}

	function toggleDbmlSourceEditor() {
		if (dbmlSourceMode) closeDbmlSourceEditor()
		else openDbmlSourceEditor()
	}

	function pullCanvasIntoEditor() {
		dbmlEditorText = diagramToDbml(diagram, diagramName)
		dbmlParseError = null
		toolMsg = 'Editor loaded from current canvas.'
	}

	function applyDbmlToCanvas() {
		const result = dbmlToDiagram(dbmlEditorText, family, diagram)
		if (!result.ok) {
			dbmlParseError = result.error
			toolMsg = result.error
			return
		}
		snapshotBeforeChange()
		diagram = result.diagram
		selectedTableIds = []
		selectedEdgeId = null
		arrowTool = null
		linkFrom = null
		draftWaypoints = []
		cursorPt = null
		showAddCol = false
		dbmlParseError = null
		markDirty()
		toolMsg = `Diagram generated from DBML (${result.diagram.nodes.length} tables, ${result.diagram.edges.length} refs).`
	}

	async function copyDbml() {
		const text = dbmlSourceMode
			? dbmlEditorText
			: dbmlText || diagramToDbml(diagram, diagramName)
		try {
			await navigator.clipboard.writeText(text)
			dbmlCopied = true
			toolMsg = 'DBML copied to clipboard.'
			setTimeout(() => {
				dbmlCopied = false
			}, 1600)
		} catch {
			toolMsg = 'Could not copy DBML — select text manually.'
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
		{#if !readOnly}
			<aside
				class={cn(
					'flex shrink-0 flex-col border-r border-line bg-panel',
					dbmlSourceMode ? 'w-[min(22rem,38vw)]' : 'w-56',
				)}
			>
				{#if dbmlSourceMode}
					<ErDbmlEditor
						bind:text={dbmlEditorText}
						parseError={dbmlParseError}
						copied={dbmlCopied}
						onclose={closeDbmlSourceEditor}
						onapply={applyDbmlToCanvas}
						onfromcanvas={pullCanvasIntoEditor}
						oncopy={() => void copyDbml()}
					/>
				{:else}
					<ErToolsPanel
						{entityWord}
						nodeLabel={meta.nodeLabel}
						bind:tableQuery
						bind:searchWrap={tableSearchWrap}
						{navNodes}
						{showGrid}
						{showDbml}
						{canUndo}
						{canRedo}
						{arrowTool}
						linkFrom={!!linkFrom}
						{selectedEdgeId}
						{toolMsg}
						edges={diagram?.edges ?? []}
						{hasNodes}
						onfocus={focusTable}
						onaddtable={addTable}
						onundo={undo}
						onredo={redo}
						ontogglegrid={toggleGrid}
						onopendbmleditor={openDbmlSourceEditor}
						ontoggledbml={toggleDbml}
						onautolayout={autoLayout}
						onsetarrow={setArrowTool}
						oncancellink={cancelLink}
						onstraighten={straightenSelectedEdge}
						onselectedge={selectEdge}
						ondeleteedge={deleteEdge}
					/>
				{/if}
			</aside>
		{:else}
			<ErObjectRail
				nodeLabel={meta.nodeLabel}
				{entityWord}
				bind:tableQuery
				bind:searchWrap={tableSearchWrap}
				{navNodes}
				totalCount={diagram?.nodes.length ?? 0}
				{selectedTableIds}
				onfocus={focusTable}
			/>
		{/if}

		{#if diagram}
			<ErCanvas
				{diagram}
				{geom}
				{edgeSpines}
				bind:canvasEl
				{worldW}
				{worldH}
				canvasW={canvasBounds.width}
				canvasH={canvasBounds.height}
				{viewZoom}
				{showGrid}
				{readOnly}
				{arrowTool}
				{entityWord}
				{selectedTableIds}
				{selectedEdgeId}
				{focusedLink}
				{fkColumnIds}
				{edgesDrawOrder}
				{dragTableId}
				{primaryTableId}
				{linkFrom}
				{cursorPt}
				{draftWaypoints}
				{typeOptions}
				{family}
				bind:showAddCol
				bind:newColName
				bind:newColType
				bind:newColPk
				oncanvasclick={onCanvasClick}
				onpointermove={onCanvasPointerMove}
				oncanvascontextmenu={onCanvasContextMenu}
				onselectedge={selectEdge}
				onmidhandledown={onMidHandleDown}
				onwaypointdown={onWaypointDown}
				onwaypointup={onWaypointUp}
				onremovewaypoint={removeWaypoint}
				ontablepointerdown={onTablePointerDown}
				ontablepointerup={onTablePointerUp}
				ontablecontextmenu={onTableContextMenu}
				oncolumnclick={onColumnClick}
				onrename={renameTable}
				onupdatetype={updateColumnType}
				ondeletecolumn={deleteColumn}
				onaddcolumn={addColumn}
			/>
		{:else}
			<div class="relative min-h-0 min-w-0 flex-1"></div>
		{/if}

		{#if showRightDbml}
			<ErDbmlSnapshot
				text={dbmlText}
				busy={dbmlBusy}
				copied={dbmlCopied}
				oncopy={() => void copyDbml()}
			/>
		{/if}
	</div>

	<ErStatusBar
		bind:diagramName
		{dirty}
		{readOnly}
		{entityWord}
		hasDiagram={!!diagram}
		nodeCount={diagram?.nodes.length ?? 0}
		edgeCount={diagram?.edges.length ?? 0}
		{selectionCount}
		{arrowTool}
		linkFrom={!!linkFrom}
		{hasNodes}
		{showGrid}
		{dbmlSourceMode}
		{showDbml}
		{canZoomOut}
		{canZoomIn}
		{zoomLabel}
		{refreshing}
		{toolMsg}
		onAutoLayout={autoLayout}
		onRefresh={onrefresh}
		onZoomOut={zoomOut}
		onZoomIn={zoomIn}
		onResetZoom={resetZoom}
		onToggleGrid={toggleGrid}
		onToggleDbmlEditor={toggleDbmlSourceEditor}
		onToggleDbml={toggleDbml}
	/>
</div>

{#if ctxMenu && !readOnly}
	<ContextMenu
		x={ctxMenu.clientX}
		y={ctxMenu.clientY}
		items={ctxMenuItems}
		ariaLabel="Diagram actions"
		onpick={onCtxPick}
		onclose={() => (ctxMenu = null)}
	/>
{/if}
