<script lang="ts">
	import { cn } from '#core/util'
	import type { Snippet } from 'svelte'
	import { onDestroy } from 'svelte'
	export type DropEdge = 'before' | 'after'

	export type DraggableApi = {
		draggingId: string | null
		dropEdge: (id: string) => DropEdge | null
		start: (e: PointerEvent, id: string) => void
		ateClick: () => boolean
	}

	interface Props {
		/** Current item ids in visual order. */
		ids: string[]
		enabled?: boolean
		/** Axis used for drop-target hit testing. Preview still follows the pointer. */
		axis?: 'x' | 'y' | 'both'
		/** Skip drag when the event target matches (e.g. a close button). */
		ignore?: string
		/** Overflow parent to auto-scroll while dragging. Also scopes item queries. */
		scrollRoot?: HTMLElement | null
		threshold?: number
		previewClass?: string
		onpickup?: (id: string) => void
		/** `beforeId` is the item to insert in front of; `null` appends. */
		onreorder?: (movedId: string, beforeId: string | null) => void
		children: Snippet<[DraggableApi]>
		preview?: Snippet<[{ id: string }]>
	}

	let {
		ids,
		enabled = true,
		axis = 'x',
		ignore,
		scrollRoot = null,
		threshold = 5,
		previewClass = '',
		onpickup,
		onreorder,
		children,
		preview,
	}: Props = $props()

	const reduceMotion =
		typeof window !== 'undefined' &&
		window.matchMedia('(prefers-reduced-motion: reduce)').matches
	const previewLift = reduceMotion ? '' : 'rotate(-2deg) scale(1.06)'

	let draggingId = $state<string | null>(null)
	let dropBeforeId = $state<string | null | undefined>(undefined)
	let previewEl = $state<HTMLDivElement | undefined>(undefined)
	let previewW = $state(0)
	let previewH = $state(0)
	let grabOffX = $state(0)
	let grabOffY = $state(0)

	let pendingId: string | null = null
	let startX = 0
	let startY = 0
	let dragStarted = false
	let activePointerId: number | null = null
	let suppressClick = false
	let previewX = 0
	let previewY = 0

	function portalToBody(node: HTMLElement) {
		document.body.appendChild(node)
		return {
			destroy() {
				node.remove()
			},
		}
	}

	function applyPreviewPos() {
		if (!previewEl) return
		previewEl.style.left = `${previewX}px`
		previewEl.style.top = `${previewY}px`
	}

	const previewStyle = $derived(
		[
			`width:${previewW}px`,
			`height:${previewH}px`,
			`left:${previewX}px`,
			`top:${previewY}px`,
			`transform-origin:${grabOffX}px ${grabOffY}px`,
			previewLift ? `transform:${previewLift}` : '',
		]
			.filter(Boolean)
			.join(';'),
	)

	function isNoop(movedId: string, beforeId: string | null): boolean {
		const from = ids.findIndex((id) => id === movedId)
		const dest =
			beforeId == null ? ids.length : ids.findIndex((id) => id === beforeId)
		return from < 0 || dest < 0 || from === dest || from + 1 === dest
	}

	function dropEdge(id: string): DropEdge | null {
		if (!draggingId || dropBeforeId === undefined) return null
		if (isNoop(draggingId, dropBeforeId)) return null
		if (dropBeforeId === id) return 'before'
		if (dropBeforeId === null && ids.at(-1) === id) return 'after'
		return null
	}

	function itemNodes(): HTMLElement[] {
		const root = scrollRoot
		if (!root) return []
		return [...root.querySelectorAll<HTMLElement>('[data-drag-id]')]
	}

	function updateDropTarget(clientX: number, clientY: number) {
		if (!draggingId) return
		const nodes = itemNodes()
		if (!nodes.length) return
		let before: string | null = null
		for (const node of nodes) {
			const id = node.dataset.dragId
			if (!id || id === draggingId) continue
			const rect = node.getBoundingClientRect()
			const hitX = axis !== 'y'
			const hitY = axis !== 'x'
			const beforeX = !hitX || clientX < rect.left + rect.width / 2
			const beforeY = !hitY || clientY < rect.top + rect.height / 2
			if (beforeX && beforeY) {
				before = id
				break
			}
		}
		dropBeforeId = before
	}

	function autoScroll(clientX: number, clientY: number) {
		const el = scrollRoot
		if (!el) return
		const rect = el.getBoundingClientRect()
		const edge = 36
		if (axis !== 'y' && el.scrollWidth > el.clientWidth) {
			if (clientX < rect.left + edge) el.scrollLeft -= 14
			else if (clientX > rect.right - edge) el.scrollLeft += 14
		}
		if (axis !== 'x' && el.scrollHeight > el.clientHeight) {
			if (clientY < rect.top + edge) el.scrollTop -= 14
			else if (clientY > rect.bottom - edge) el.scrollTop += 14
		}
	}

	function movePreview(clientX: number, clientY: number) {
		previewX = clientX - grabOffX
		previewY = clientY - grabOffY
		applyPreviewPos()
	}

	function beginPreview(id: string, clientX: number, clientY: number) {
		const node = itemNodes().find((el) => el.dataset.dragId === id)
		const rect = node?.getBoundingClientRect()
		if (rect) {
			grabOffX = clientX - rect.left
			grabOffY = clientY - rect.top
			previewW = rect.width
			previewH = rect.height
			previewX = rect.left
			previewY = rect.top
		} else {
			grabOffX = 48
			grabOffY = 12
			previewW = 160
			previewH = 28
			movePreview(clientX, clientY)
		}
	}

	function endDrag(commit: boolean) {
		const moved = draggingId
		const before = dropBeforeId
		document.body.style.userSelect = ''
		document.body.style.cursor = ''
		pendingId = null
		dragStarted = false
		activePointerId = null
		draggingId = null
		dropBeforeId = undefined
		previewW = 0
		previewH = 0
		if (
			commit &&
			moved &&
			before !== undefined &&
			onreorder &&
			!isNoop(moved, before)
		) {
			onreorder(moved, before)
		}
	}

	function start(e: PointerEvent, id: string) {
		if (!enabled || e.button !== 0 || ids.length < 2) return
		if (ignore && (e.target as HTMLElement).closest(ignore)) return
		pendingId = id
		startX = e.clientX
		startY = e.clientY
		dragStarted = false
		activePointerId = e.pointerId
	}

	function onPointerMove(e: PointerEvent) {
		if (pendingId == null || e.pointerId !== activePointerId) return
		if (!ids.includes(pendingId)) {
			endDrag(false)
			return
		}
		if (!dragStarted) {
			const dx = e.clientX - startX
			const dy = e.clientY - startY
			const dist =
				axis === 'x'
					? Math.abs(dx)
					: axis === 'y'
						? Math.abs(dy)
						: Math.hypot(dx, dy)
			if (dist < threshold) return
			dragStarted = true
			suppressClick = true
			draggingId = pendingId
			dropBeforeId = pendingId
			document.body.style.userSelect = 'none'
			document.body.style.cursor = 'grabbing'
			beginPreview(pendingId, e.clientX, e.clientY)
			onpickup?.(pendingId)
		}
		movePreview(e.clientX, e.clientY)
		autoScroll(e.clientX, e.clientY)
		updateDropTarget(e.clientX, e.clientY)
	}

	function onPointerUp(e: PointerEvent) {
		if (e.pointerId !== activePointerId) return
		endDrag(dragStarted)
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && draggingId) {
			e.preventDefault()
			endDrag(false)
		}
	}

	function ateClick(): boolean {
		if (!suppressClick) return false
		suppressClick = false
		return true
	}

	const api: DraggableApi = {
		get draggingId() {
			return draggingId
		},
		dropEdge,
		start,
		ateClick,
	}

	onDestroy(() => {
		if (pendingId != null || draggingId) endDrag(false)
	})
</script>

<svelte:window
	onkeydown={onKeydown}
	onpointermove={onPointerMove}
	onpointerup={onPointerUp}
	onpointercancel={onPointerUp}
/>

{@render children(api)}

{#if draggingId && previewW > 0 && preview}
	<div
		bind:this={previewEl}
		use:portalToBody
		class={cn(
			'pointer-events-none fixed z-50 flex overflow-hidden rounded-md border border-line-strong bg-elevated shadow-md',
			previewClass,
		)}
		style={previewStyle}
		aria-hidden="true"
	>
		{@render preview({ id: draggingId })}
	</div>
{/if}
