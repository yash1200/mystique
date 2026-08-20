<script lang="ts">
	/**
	 * Windowed (virtualized) results table.
	 * Large directional overscan keeps rows ahead of the scroll so spacers
	 * rarely flash empty while the user is moving through big result sets.
	 */
	import type { QueryResult } from '$lib/api/engines'
	import { appPrefs } from '$lib/stores/app-prefs.svelte'
	import { cn } from '$lib/utils/cn'

	/** Virtualization row height — follows density pref / `--grid-row-height`. */
	const ROW_H = $derived(appPrefs.gridRowHeightPx)
	const HEADER_H = 32
	/** Minimum rows buffered on each side of the viewport. */
	const OVERSCAN_MIN = 200
	/**
	 * Extra buffer as multiples of the visible row count.
	 * ~2.5 screens ahead of motion absorbs fast wheel / trackpad flings
	 * without mounting tens of thousands of nodes.
	 */
	const OVERSCAN_VIEWPORTS = 2.5
	/** Extra multiplier applied in the direction the user is scrolling. */
	const DIRECTION_BIAS = 1.75

	interface Props {
		columns: QueryResult['columns']
		/** Rows for the current page (may be up to 50k). */
		rows: unknown[][]
		/** Absolute index of rows[0] in the full result set (for # column). */
		pageStart: number
	}

	let { columns, rows, pageStart }: Props = $props()

	let scrollEl = $state<HTMLDivElement | null>(null)
	let scrollTop = $state(0)
	let viewportH = $state(360)
	/** +1 scrolling down, -1 scrolling up. */
	let scrollDir = $state(1)

	const colCount = $derived(columns.length + 1)
	const totalRows = $derived(rows.length)

	const windowRange = $derived.by(() => {
		const n = totalRows
		if (n === 0) return { start: 0, end: 0, padTop: 0, padBottom: 0 }

		const usable = Math.max(
			viewportH > HEADER_H ? viewportH - HEADER_H : viewportH,
			ROW_H * 8,
		)
		const first = Math.max(0, Math.floor(scrollTop / ROW_H))
		const visibleCount = Math.ceil(usable / ROW_H) + 1

		const baseOverscan = Math.max(
			OVERSCAN_MIN,
			Math.ceil(visibleCount * OVERSCAN_VIEWPORTS),
		)
		// Prefer buffer ahead of the scroll so fling/trackpad lag stays covered.
		const overscanBefore =
			scrollDir < 0 ? Math.ceil(baseOverscan * DIRECTION_BIAS) : baseOverscan
		const overscanAfter =
			scrollDir > 0 ? Math.ceil(baseOverscan * DIRECTION_BIAS) : baseOverscan

		const start = Math.max(0, first - overscanBefore)
		const end = Math.min(n, first + visibleCount + overscanAfter)
		return {
			start,
			end,
			padTop: start * ROW_H,
			padBottom: Math.max(0, (n - end) * ROW_H),
		}
	})

	const windowRows = $derived.by(() => {
		const { start, end } = windowRange
		const out: { absolute: number; cells: unknown[] }[] = []
		for (let i = start; i < end; i++) {
			out.push({ absolute: pageStart + i, cells: rows[i]! })
		}
		return out
	})

	function onScroll() {
		const el = scrollEl
		if (!el) return
		const next = el.scrollTop
		// Ignore sub-pixel jitter; still update direction on meaningful moves.
		if (next > scrollTop + 1) scrollDir = 1
		else if (next < scrollTop - 1) scrollDir = -1
		// Apply immediately so the window moves with the thumb, not a frame later.
		scrollTop = next
	}

	$effect(() => {
		const el = scrollEl
		if (!el) return
		const measure = () => {
			viewportH = el.clientHeight
		}
		measure()
		if (typeof ResizeObserver === 'undefined') return
		const ro = new ResizeObserver(measure)
		ro.observe(el)
		return () => ro.disconnect()
	})

	// Reset window when page / dataset changes
	$effect(() => {
		void pageStart
		void totalRows
		void columns.length
		scrollTop = 0
		scrollDir = 1
		const el = scrollEl
		if (el) el.scrollTop = 0
	})

	function cellDisplay(v: unknown): string {
		if (v === null || v === undefined) return 'NULL'
		if (typeof v === 'object') {
			try {
				return JSON.stringify(v)
			} catch {
				return String(v)
			}
		}
		return String(v)
	}
</script>

<div
	class="min-h-0 flex-1 overflow-auto"
	bind:this={scrollEl}
	onscroll={onScroll}
	data-query-results-scroll
>
	<table class="w-max min-w-full border-collapse text-left">
		<thead class="sticky top-0 z-10 bg-elevated">
			<tr style="height: {HEADER_H}px">
				<th
					class="border-b border-line px-2 py-1.5 font-mono text-caption font-semibold text-fg-muted"
					>#</th
				>
				{#each columns as col, ci (col.name + '\0' + ci)}
					<th
						class="border-b border-line px-2.5 py-1.5 font-mono text-caption font-semibold whitespace-nowrap text-fg-secondary"
						title={col.dataType ?? undefined}
					>
						{col.name}
						{#if col.dataType}
							<span class="ml-1 font-normal text-fg-muted">{col.dataType}</span>
						{/if}
					</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#if windowRange.padTop > 0}
				<tr aria-hidden="true" class="pointer-events-none">
					<td
						colspan={colCount}
						class="border-0 p-0"
						style="height: {windowRange.padTop}px; line-height: 0; font-size: 0"
					>
						<div style="height: {windowRange.padTop}px"></div>
					</td>
				</tr>
			{/if}

			{#each windowRows as item (item.absolute)}
				<tr class="hover:bg-hover" style="height: {ROW_H}px">
					<td
						class="border-b border-line/60 px-2 align-middle font-mono text-caption text-fg-muted tabular-nums"
						style="height: {ROW_H}px; max-height: {ROW_H}px; padding-top: 0; padding-bottom: 0"
					>
						{item.absolute + 1}
					</td>
					{#each item.cells as cell, ci (ci)}
						{@const isNull = cell === null || cell === undefined}
						{@const text = cellDisplay(cell)}
						<td
							data-result-cell
							class={cn(
								'max-w-xs truncate border-b border-line/60 px-2.5 align-middle font-mono text-label',
								isNull ? 'italic text-fg-muted' : 'text-fg',
							)}
							style="height: {ROW_H}px; max-height: {ROW_H}px; padding-top: 0; padding-bottom: 0"
							title={text}
						>
							{text}
						</td>
					{/each}
				</tr>
			{/each}

			{#if windowRange.padBottom > 0}
				<tr aria-hidden="true" class="pointer-events-none">
					<td
						colspan={colCount}
						class="border-0 p-0"
						style="height: {windowRange.padBottom}px; line-height: 0; font-size: 0"
					>
						<div style="height: {windowRange.padBottom}px"></div>
					</td>
				</tr>
			{/if}
		</tbody>
	</table>
</div>
