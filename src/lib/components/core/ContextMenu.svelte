<script lang="ts">
	import { cn } from '$lib/utils/cn'
	import { popover } from '$lib/utils/motion'
	import { onMount } from 'svelte'

	export type ContextMenuItem = {
		id: string
		label: string
		/** Optional keyboard hint shown on the right (e.g. ⌘D). */
		shortcut?: string
		danger?: boolean
		disabled?: boolean
		separatorBefore?: boolean
		/** Non-interactive section title. */
		heading?: boolean
		/** Current choice in a one-of list. */
		checked?: boolean
	}

	interface Props {
		x: number
		y: number
		items: ContextMenuItem[]
		ariaLabel?: string
		onpick: (id: string) => void
		onclose: () => void
	}

	let { x, y, items, ariaLabel = 'Actions', onpick, onclose }: Props = $props()

	let root = $state<HTMLDivElement | null>(null)
	let left = $state(0)
	let top = $state(0)

	onMount(() => {
		left = x
		top = y
		const el = root
		if (el) {
			const rect = el.getBoundingClientRect()
			const pad = 8
			left = Math.max(pad, Math.min(x, window.innerWidth - rect.width - pad))
			top = Math.max(pad, Math.min(y, window.innerHeight - rect.height - pad))
		}
		const onDoc = (e: PointerEvent) => {
			if (e.target instanceof Node && root?.contains(e.target)) return
			onclose()
		}
		const onKey = (e: KeyboardEvent) => {
			if (e.key === 'Escape') onclose()
		}
		// Wait a tick so the opening right-click cannot close the menu.
		const tid = window.setTimeout(() => {
			document.addEventListener('pointerdown', onDoc, true)
		}, 0)
		window.addEventListener('keydown', onKey)
		return () => {
			clearTimeout(tid)
			document.removeEventListener('pointerdown', onDoc, true)
			window.removeEventListener('keydown', onKey)
		}
	})

	function pick(id: string, disabled?: boolean) {
		if (disabled) return
		onpick(id)
	}
</script>

<div
	bind:this={root}
	class="fixed z-50 min-w-44 rounded-md border border-line bg-elevated py-1 shadow-md"
	style="left:{left}px; top:{top}px"
	transition:popover|global={{ origin: 'top left' }}
	role="menu"
	tabindex="-1"
	aria-label={ariaLabel}
	onpointerdown={(e) => e.stopPropagation()}
	oncontextmenu={(e) => e.preventDefault()}
>
	{#each items as item (item.id)}
		{#if item.separatorBefore}
			<div class="my-1 border-t border-line" role="separator"></div>
		{/if}
		{#if item.heading}
			<p
				class="m-0 px-2.5 py-1 text-micro font-semibold tracking-wide text-fg-muted uppercase"
			>
				{item.label}
			</p>
		{:else}
			<button
				type="button"
				role="menuitem"
				disabled={item.disabled}
				class={cn(
					'flex w-full cursor-pointer items-center gap-4 border-none bg-transparent px-2.5 py-1.5 text-left text-label',
					item.danger
						? 'text-danger hover:bg-danger/10'
						: item.checked
							? 'bg-accent-muted text-accent'
							: 'text-fg hover:bg-hover',
					item.disabled && 'cursor-not-allowed opacity-45 hover:bg-transparent',
				)}
				onpointerdown={(e) => {
					e.stopPropagation()
					pick(item.id, item.disabled)
				}}
			>
				<span class="min-w-0 flex-1">{item.label}</span>
				{#if item.checked}
					<svg
						class="size-3.5 shrink-0"
						viewBox="0 0 16 16"
						fill="none"
						aria-hidden="true"
					>
						<path
							d="M3.5 8.5l3 3 6-6.5"
							stroke="currentColor"
							stroke-width="1.5"
							stroke-linecap="round"
							stroke-linejoin="round"
						/>
					</svg>
				{:else if item.shortcut}
					<span
						class="shrink-0 font-mono text-micro text-fg-muted tabular-nums"
					>
						{item.shortcut}
					</span>
				{/if}
			</button>
		{/if}
	{/each}
</div>
