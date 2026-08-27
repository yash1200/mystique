<script lang="ts">
	import { EmptyState } from '$lib/components/core'
	import type { DataFamily, ErDiagramState, ErEdge, ErPoint } from '$lib/types/er'
	import { ER_GRID } from '$lib/types/er'
	import { isBrokenEdge, type ErFocusedLink } from './erConstants'
	import type { EdgeSpine, ErGeometry } from './erGeometry'
	import ErTableCard from './ErTableCard.svelte'

	interface Props {
		diagram: ErDiagramState
		geom: ErGeometry
		edgeSpines: Map<string, EdgeSpine>
		canvasEl: HTMLDivElement | null
		canvasW: number
		canvasH: number
		viewZoom: number
		/** Live drag positions (unsnapped). Cards use this so committed state stays still. */
		dragPreview?: Record<string, { x: number; y: number }> | null
		showGrid: boolean
		readOnly: boolean
		arrowTool: 'normal' | 'broken' | null
		entityWord: string
		selectedTableIds: string[]
		selectedEdgeId: string | null
		focusedLink: ErFocusedLink | null
		fkColumnIds: Set<string>
		edgesDrawOrder: ErEdge[]
		dragTableId: string | null
		primaryTableId: string | null
		linkFrom: { tableId: string; columnId: string } | null
		cursorPt: ErPoint | null
		draftWaypoints: ErPoint[]
		typeOptions: { value: string; label: string }[]
		family: DataFamily
		showAddCol: boolean
		newColName: string
		newColType: string
		newColPk: boolean
		oncanvasclick: (e: MouseEvent) => void
		onpointermove: (e: PointerEvent) => void
		oncanvascontextmenu: (e: MouseEvent) => void
		onselectedge: (edgeId: string, e?: Event) => void
		onmidhandledown: (edgeId: string, insertAt: number, e: PointerEvent) => void
		onwaypointdown: (edgeId: string, index: number, e: PointerEvent) => void
		onwaypointup: (e: PointerEvent) => void
		onremovewaypoint: (edgeId: string, index: number, e: Event) => void
		ontablepointerdown: (tableId: string, e: PointerEvent) => void
		ontablepointerup: (e: PointerEvent) => void
		ontablecontextmenu: (tableId: string, e: MouseEvent) => void
		oncolumnclick: (tableId: string, columnId: string, e: MouseEvent) => void
		onrename: (name: string) => void
		onupdatetype: (tableId: string, colId: string, dataType: string) => void
		ondeletecolumn: (colId: string) => void
		onaddcolumn: () => void
	}

	let {
		diagram,
		geom,
		edgeSpines,
		canvasEl = $bindable<HTMLDivElement | null>(null),
		canvasW,
		canvasH,
		viewZoom,
		dragPreview = null,
		showGrid,
		readOnly,
		arrowTool,
		entityWord,
		selectedTableIds,
		selectedEdgeId,
		focusedLink,
		fkColumnIds,
		edgesDrawOrder,
		dragTableId,
		primaryTableId,
		linkFrom,
		cursorPt,
		draftWaypoints,
		typeOptions,
		family,
		showAddCol = $bindable(false),
		newColName = $bindable(''),
		newColType = $bindable(''),
		newColPk = $bindable(false),
		oncanvasclick,
		onpointermove,
		oncanvascontextmenu,
		onselectedge,
		onmidhandledown,
		onwaypointdown,
		onwaypointup,
		onremovewaypoint,
		ontablepointerdown,
		ontablepointerup,
		ontablecontextmenu,
		oncolumnclick,
		onrename,
		onupdatetype,
		ondeletecolumn,
		onaddcolumn,
	}: Props = $props()

	let viewW = $state(0)
	let viewH = $state(0)

	$effect(() => {
		const el = canvasEl
		if (!el) return
		const apply = () => {
			viewW = el.clientWidth
			viewH = el.clientHeight
		}
		apply()
		const ro = new ResizeObserver(apply)
		ro.observe(el)
		return () => ro.disconnect()
	})

	const displayDiagram = $derived.by(() => {
		if (!dragPreview) return diagram
		return {
			...diagram,
			nodes: diagram.nodes.map((n) => {
				const p = dragPreview[n.id]
				return p ? { ...n, x: p.x, y: p.y } : n
			}),
		}
	})

	const layerW = $derived(
		Math.max(canvasW, viewZoom > 0 ? viewW / viewZoom : viewW, 1),
	)
	const layerH = $derived(
		Math.max(canvasH, viewZoom > 0 ? viewH / viewZoom : viewH, 1),
	)
	const worldW = $derived(Math.max(1, Math.round(layerW * viewZoom)))
	const worldH = $derived(Math.max(1, Math.round(layerH * viewZoom)))

	const previewPath = $derived(
		linkFrom && cursorPt
			? geom.previewPolyline(displayDiagram, linkFrom, draftWaypoints, cursorPt)
			: '',
	)
</script>

<div class="relative min-h-0 min-w-0 flex-1">
	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<div
		class="absolute inset-0 touch-none overflow-auto overscroll-none bg-app-wash"
		style="cursor: {readOnly ? 'default' : arrowTool ? 'crosshair' : 'default'};"
		bind:this={canvasEl}
		role="application"
		aria-label={readOnly ? 'Live ER diagram (read-only)' : 'ER grid canvas'}
		tabindex="0"
		onclick={oncanvasclick}
		{onpointermove}
		oncontextmenu={oncanvascontextmenu}
	>
		<div
			class="relative min-h-full min-w-full"
			style="width: {worldW}px; height: {worldH}px;"
		>
			<div
				class="absolute top-0 left-0 origin-top-left"
				style="
					width: {layerW}px;
					height: {layerH}px;
					transform: scale({viewZoom});
					background-image: {showGrid
					? `linear-gradient(to right, color-mix(in srgb, var(--border-subtle) 70%, transparent) 1px, transparent 1px),
						linear-gradient(to bottom, color-mix(in srgb, var(--border-subtle) 70%, transparent) 1px, transparent 1px)`
					: 'none'};
					background-size: {ER_GRID}px {ER_GRID}px;
				"
			>
				<svg
					class="absolute top-0 left-0"
					style="width: {layerW}px; height: {layerH}px; pointer-events: none;"
				>
					<defs>
						<marker
							id="er-arrow-head"
							viewBox="0 0 14 14"
							markerWidth="14"
							markerHeight="14"
							refX="12"
							refY="7"
							orient="auto"
							markerUnits="userSpaceOnUse"
						>
							<path
								d="M1,1 L13,7 L1,13 L4,7 Z"
								fill="var(--accent)"
								stroke="var(--accent)"
								stroke-width="0.5"
							/>
						</marker>
						<marker
							id="er-arrow-head-sel"
							viewBox="0 0 16 16"
							markerWidth="16"
							markerHeight="16"
							refX="14"
							refY="8"
							orient="auto"
							markerUnits="userSpaceOnUse"
						>
							<path
								d="M1,1 L15,8 L1,15 L4.5,8 Z"
								fill="var(--accent)"
								stroke="var(--accent-hover)"
								stroke-width="1"
							/>
						</marker>
						<marker
							id="er-arrow-tail"
							viewBox="0 0 10 10"
							markerWidth="8"
							markerHeight="8"
							refX="5"
							refY="5"
							orient="auto"
							markerUnits="userSpaceOnUse"
						>
							<circle
								cx="5"
								cy="5"
								r="3.5"
								fill="var(--accent)"
								stroke="var(--accent-hover)"
								stroke-width="1"
							/>
						</marker>
					</defs>

					{#each edgesDrawOrder as edge (edge.id)}
						{@const selected = selectedEdgeId === edge.id}
						{@const edgeDim = !!focusedLink && !selected}
						{@const d = geom.edgePolyline(displayDiagram, edgeSpines, edge)}
						<path
							data-er-edge-hit
							{d}
							fill="none"
							stroke="transparent"
							stroke-width="16"
							style="pointer-events: stroke; cursor: pointer;"
							role="button"
							tabindex="-1"
							aria-label="Select relationship {edge.label}"
							onclick={(e) => onselectedge(edge.id, e)}
							onkeydown={(e) => {
								if (e.key === 'Enter' || e.key === ' ') {
									e.preventDefault()
									onselectedge(edge.id, e)
								}
							}}
						/>
						<path
							{d}
							fill="none"
							stroke="var(--accent)"
							stroke-width={selected ? 3.25 : 2.25}
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-dasharray={isBrokenEdge(edge.kind) ? '8 5' : '0'}
							marker-start="url(#er-arrow-tail)"
							marker-end={selected
								? 'url(#er-arrow-head-sel)'
								: 'url(#er-arrow-head)'}
							opacity={edgeDim ? 0.12 : selected ? 1 : 0.95}
							style="pointer-events: none;"
						/>

						{#if selected && !arrowTool}
							{#each geom.segmentMids(displayDiagram, edgeSpines, edge) as mid (mid.insertAt + '-' + mid.x + '-' + mid.y)}
								<g
									role="button"
									tabindex="-1"
									aria-label="Add edge waypoint"
									data-er-edge-handle
									style="pointer-events: all; cursor: move;"
									onpointerdown={(e) => onmidhandledown(edge.id, mid.insertAt, e)}
									onpointerup={onwaypointup}
									onpointercancel={onwaypointup}
								>
									<rect
										x={mid.x - 3.5}
										y={mid.y - 3.5}
										width="7"
										height="7"
										transform="rotate(45 {mid.x} {mid.y})"
										fill="var(--bg-panel, #1a1a1a)"
										stroke="var(--accent)"
										stroke-width="1.25"
									/>
								</g>
							{/each}

							{#each edge.waypoints ?? [] as wp, wi (edge.id + '-wp-' + wi)}
								<g
									role="button"
									tabindex="-1"
									aria-label="Drag edge waypoint (double-click to remove)"
									data-er-edge-handle
									style="pointer-events: all; cursor: grab;"
									onpointerdown={(e) => onwaypointdown(edge.id, wi, e)}
									onpointerup={onwaypointup}
									onpointercancel={onwaypointup}
									ondblclick={(e) => onremovewaypoint(edge.id, wi, e)}
								>
									<circle
										cx={wp.x}
										cy={wp.y}
										r="4.5"
										fill="var(--accent)"
										stroke="var(--bg-panel, #fff)"
										stroke-width="1.5"
									/>
								</g>
							{/each}
						{/if}
					{/each}

					{#if linkFrom && cursorPt}
						<path
							d={previewPath}
							fill="none"
							stroke="var(--accent)"
							stroke-width="2.25"
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-dasharray="6 4"
							opacity="0.85"
							marker-start="url(#er-arrow-tail)"
							marker-end="url(#er-arrow-head)"
							style="pointer-events: none;"
						/>
						{#each draftWaypoints as wp, i (`${wp.x},${wp.y},${i}`)}
							<circle
								cx={wp.x}
								cy={wp.y}
								r="4"
								fill="var(--accent)"
								style="pointer-events: none;"
							/>
							<text
								x={wp.x + 6}
								y={wp.y - 6}
								fill="var(--text-muted)"
								font-size="10"
								style="pointer-events: none;">{i + 1}</text
							>
						{/each}
					{/if}
				</svg>

				{#each diagram.nodes as node (node.id)}
					{@const isSelected = selectedTableIds.includes(node.id)}
					{@const inLinkFocus = !!focusedLink?.tableIds.has(node.id)}
					<ErTableCard
						{node}
						preview={dragPreview?.[node.id] ?? null}
						tableW={geom.tableW}
						headerH={geom.headerH}
						rowH={geom.rowH}
						{isSelected}
						{inLinkFocus}
						dimmed={!!focusedLink && !inLinkFocus}
						{readOnly}
						{arrowTool}
						isPrimary={primaryTableId === node.id}
						isDragging={dragTableId === node.id}
						{fkColumnIds}
						focusedColIds={focusedLink?.colIds ?? null}
						{linkFrom}
						{typeOptions}
						{family}
						bind:showAddCol
						bind:newColName
						bind:newColType
						bind:newColPk
						onpointerdown={(e) => ontablepointerdown(node.id, e)}
						{onpointermove}
						onpointerup={ontablepointerup}
						oncontextmenu={(e) => ontablecontextmenu(node.id, e)}
						oncolumnclick={(colId, e) => oncolumnclick(node.id, colId, e)}
						{onrename}
						onupdatetype={(colId, dataType) =>
							onupdatetype(node.id, colId, dataType)}
						{ondeletecolumn}
						{onaddcolumn}
					/>
				{/each}
			</div>
		</div>

		{#if diagram.nodes.length === 0}
			<div
				class="pointer-events-none absolute inset-0 flex items-center justify-center p-8"
			>
				<EmptyState
					title="No {entityWord}s to map"
					description={readOnly
						? 'This table has no columns or related objects to draw yet.'
						: `Add a ${entityWord} from the toolbar, or open the DBML editor and apply a schema.`}
				/>
			</div>
		{/if}
	</div>
</div>
