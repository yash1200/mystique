<script lang="ts">
	import { cn, popover } from '#core/util'
	import type { IconName } from '#icons'
	import { onMount } from 'svelte'
	import Icon from './Icon.svelte'

	export type ContextMenuItem = {
		id: string
		label: string
		/** Optional leading glyph. Same family as the rest of the app. */
		icon?: IconName
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
		/** Mono label above the items: what the menu acts on (`schema.table`). */
		title?: string
		/**
		 * `cursor` — (x, y) is the top-left (right-click).
		 * `above-end` — (x, y) is the bottom-right, so a status-bar
		 * trigger stays visible and cannot eat the opening click.
		 */
		placement?: 'cursor' | 'above-end'
		onpick: (id: string) => void
		onclose: () => void
	}

	let {
		x,
		y,
		items,
		ariaLabel = 'Actions',
		title,
		placement = 'cursor',
		onpick,
		onclose,
	}: Props = $props()

	let root = $state<HTMLDivElement | null>(null)
	let left = $state(0)
	let top = $state(0)

	onMount(() => {
		const el = root
		const pad = 8
		if (el) {
			const box = el.getBoundingClientRect()
			const rawLeft = placement === 'above-end' ? x - box.width : x
			const rawTop = placement === 'above-end' ? y - box.height : y
			left = Math.max(
				pad,
				Math.min(rawLeft, window.innerWidth - box.width - pad),
			)
			top = Math.max(
				pad,
				Math.min(rawTop, window.innerHeight - box.height - pad),
			)
		} else {
			left = x
			top = y
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
	class="fixed z-50 min-w-56 rounded-[11px] border border-line bg-raised p-[5px] shadow-md"
	style="left:{left}px; top:{top}px"
	transition:popover|global={{
		origin: placement === 'above-end' ? 'bottom right' : 'top left',
	}}
	role="menu"
	tabindex="-1"
	aria-label={ariaLabel}
	onpointerdown={(e) => e.stopPropagation()}
	oncontextmenu={(e) => e.preventDefault()}
>
	{#if title}
		<p class="m-0 truncate px-[9px] pt-1.5 pb-[5px] font-mono text-[0.71875rem] text-fg-muted">
			{title}
		</p>
	{/if}
	{#each items as item (item.id)}
		{#if item.separatorBefore}
			<div class="mx-1 my-[5px] h-px bg-line" role="separator"></div>
		{/if}
		{#if item.heading}
			<p
				class="m-0 px-2.5 pt-1.5 pb-1 label-caps"
			>
				{item.label}
			</p>
		{:else}
			<button
				type="button"
				role="menuitem"
				disabled={item.disabled}
				class={cn(
					'group/mi flex h-[1.875rem] w-full cursor-pointer items-center gap-2.5 rounded-[7px] border-none bg-transparent px-[9px] text-left text-body whitespace-nowrap',
					item.danger
						? 'text-danger hover:bg-danger-soft'
						: 'text-fg hover:bg-hover',
					item.disabled && 'cursor-not-allowed opacity-45 hover:bg-transparent',
				)}
				onpointerdown={(e) => {
					e.stopPropagation()
					pick(item.id, item.disabled)
				}}
			>
				{#if item.icon}
					<span class={cn('flex [&_svg]:size-3.5', item.danger ? 'text-danger' : 'text-fg-muted')}>
						<Icon name={item.icon} />
					</span>
				{/if}
				<span class="min-w-0 flex-1">{item.label}</span>
				{#if item.checked}
					<span class="flex text-accent-text [&_svg]:size-3.5"><Icon name="check" /></span>
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
