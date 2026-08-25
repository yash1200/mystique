<script lang="ts">
	import { cn } from '$lib/utils/cn'
	import type { Snippet } from 'svelte'

	type Axis = 'x' | 'y'
	type Edge = 'start' | 'end'
	type Variant = 'overlay' | 'split'

	interface Props {
		size: number
		defaultSize: number
		min?: number
		max?: number
		step?: number
		axis?: Axis
		/** Handle side. `start` inverts drag so the panel grows toward that edge. */
		edge?: Edge
		/** `overlay` sits on a panel edge. `split` is an in-flow sash. */
		variant?: Variant
		/** When set, drag delta is a percent of this element's size on `axis`. */
		track?: HTMLElement | null
		label: string
		title?: string
		dragging?: boolean
		onresize?: (next: number) => void
		oncommit?: (next: number) => void
		class?: string
		children?: Snippet
		[key: `data-${string}`]: string | boolean | undefined
	}

	let {
		size,
		defaultSize,
		min,
		max,
		step = 12,
		axis = 'x',
		edge = 'end',
		variant = 'overlay',
		track = null,
		label,
		title = 'Drag to resize · double-click to reset',
		dragging = $bindable(false),
		onresize,
		oncommit,
		class: className = '',
		children,
		...rest
	}: Props = $props()

	function clamp(n: number): number {
		let v = n
		if (min != null) v = Math.max(min, v)
		if (max != null) v = Math.min(max, v)
		return v
	}

	function apply(next: number, commit: boolean) {
		const v = clamp(next)
		onresize?.(v)
		if (commit) oncommit?.(v)
	}

	function dir(): number {
		return edge === 'start' ? -1 : 1
	}

	function onPointerDown(e: PointerEvent) {
		if (e.button !== 0) return
		e.preventDefault()
		const startPtr = axis === 'x' ? e.clientX : e.clientY
		const startSize = size
		const trackLen =
			track == null
				? 0
				: axis === 'x'
					? track.getBoundingClientRect().width
					: track.getBoundingClientRect().height
		if (track != null ? trackLen <= 0 : variant === 'split') return
		dragging = true
		const target = e.currentTarget as HTMLElement
		target.setPointerCapture(e.pointerId)
		document.body.style.cursor = axis === 'x' ? 'col-resize' : 'row-resize'
		document.body.style.userSelect = 'none'

		const onMove = (ev: PointerEvent) => {
			const ptr = axis === 'x' ? ev.clientX : ev.clientY
			const raw = ptr - startPtr
			const delta = track != null ? (raw / trackLen) * 100 : raw
			apply(startSize + dir() * delta, false)
		}
		const onUp = (ev: PointerEvent) => {
			dragging = false
			document.body.style.cursor = ''
			document.body.style.userSelect = ''
			try {
				target.releasePointerCapture(ev.pointerId)
			} catch {
				/* ignore */
			}
			target.removeEventListener('pointermove', onMove)
			target.removeEventListener('pointerup', onUp)
			target.removeEventListener('pointercancel', onUp)
			oncommit?.(size)
		}
		target.addEventListener('pointermove', onMove)
		target.addEventListener('pointerup', onUp)
		target.addEventListener('pointercancel', onUp)
	}

	function nudge(stepDir: -1 | 1) {
		apply(size + dir() * stepDir * step, true)
	}

	function onKeydown(e: KeyboardEvent) {
		if (axis === 'x') {
			if (e.key === 'ArrowLeft') {
				e.preventDefault()
				nudge(-1)
				return
			}
			if (e.key === 'ArrowRight') {
				e.preventDefault()
				nudge(1)
				return
			}
		} else {
			if (e.key === 'ArrowUp') {
				e.preventDefault()
				nudge(-1)
				return
			}
			if (e.key === 'ArrowDown') {
				e.preventDefault()
				nudge(1)
				return
			}
		}
		if (e.key === 'Home') {
			e.preventDefault()
			apply(defaultSize, true)
		}
	}

	const handlePos = $derived(
		variant === 'split'
			? ''
			: axis === 'x'
				? edge === 'end'
					? 'top-0 right-0 h-full w-2 translate-x-1/2'
					: 'top-0 left-0 h-full w-2 -translate-x-1/2'
				: edge === 'end'
					? 'bottom-0 left-0 h-2 w-full translate-y-1/2'
					: 'top-0 left-0 h-2 w-full -translate-y-1/2',
	)

	const handleClass = $derived(
		variant === 'split'
			? cn(
					'group relative z-10 flex shrink-0 items-center justify-center border-line bg-elevated p-0 hover:bg-hover',
					axis === 'y'
						? 'h-1.5 w-full cursor-row-resize border-x-0 border-y'
						: 'h-full w-1.5 cursor-col-resize border-y-0 border-x',
					dragging && 'bg-accent-muted',
				)
			: cn(
					'group absolute z-10 flex shrink-0 items-center justify-center border-none bg-transparent p-0',
					axis === 'x' ? 'cursor-col-resize' : 'cursor-row-resize',
					handlePos,
					'hover:bg-hover/80',
					dragging && 'bg-accent-muted',
				),
	)

	const gripClass = $derived(
		variant === 'split'
			? cn(
					'pointer-events-none rounded-full bg-line-strong opacity-70 group-hover:bg-accent group-hover:opacity-100',
					axis === 'y' ? 'h-0.5 w-10' : 'h-10 w-0.5',
					dragging && 'bg-accent opacity-100',
				)
			: cn(
					'pointer-events-none rounded-full bg-line-strong opacity-0 transition-opacity duration-150 ease-out',
					'group-hover:opacity-100 group-focus-visible:opacity-100',
					axis === 'y' ? 'h-0.5 w-10' : 'h-10 w-0.5',
					dragging && 'bg-accent opacity-100',
				),
	)

	const unit = $derived(track != null ? '%' : 'px')
</script>

{#snippet handle()}
	<button
		type="button"
		role={variant === 'split' ? 'slider' : undefined}
		aria-orientation={variant === 'split'
			? axis === 'y'
				? 'vertical'
				: 'horizontal'
			: undefined}
		aria-valuemin={variant === 'split' ? min : undefined}
		aria-valuemax={variant === 'split' ? max : undefined}
		aria-valuenow={variant === 'split' ? Math.round(size) : undefined}
		aria-label="{label} ({Math.round(size)}{unit})"
		{title}
		class={handleClass}
		onpointerdown={onPointerDown}
		ondblclick={() => apply(defaultSize, true)}
		onkeydown={onKeydown}
	>
		<span class={gripClass} aria-hidden="true"></span>
	</button>
{/snippet}

{#if children}
	<div
		class={cn('relative flex h-full shrink-0 flex-col', className)}
		style={axis === 'x' ? `width: ${size}px` : `height: ${size}px`}
		{...rest}
	>
		<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
			{@render children()}
		</div>
		{@render handle()}
	</div>
{:else}
	{@render handle()}
{/if}
