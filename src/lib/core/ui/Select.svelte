<script lang="ts">
	/**
	 * Custom select — same look on every OS (no native <select> menu).
	 * Menu is position:fixed so it isn’t clipped by overflow parents.
	 */
	import { cn, popover } from '#core/util'
	import type { Snippet } from 'svelte'
	import Icon from './Icon.svelte'

	export type SelectOption = {
		value: string
		label: string
		disabled?: boolean
	}

	interface Props {
		value?: string
		options?: SelectOption[]
		placeholder?: string
		disabled?: boolean
		inputSize?: 'sm' | 'md'
		/** `field` is a full-width control. `quiet` is a compact chip. */
		variant?: 'field' | 'quiet'
		/** Closed-state label. Menu options still use `options[].label`. */
		triggerLabel?: string
		ariaLabel?: string
		class?: string
		id?: string
		/** Fires after the bound value changes. */
		onchange?: (value: string) => void
		leading?: Snippet
	}

	let {
		value = $bindable(''),
		options = [],
		placeholder = 'Select…',
		disabled = false,
		inputSize = 'md',
		variant = 'field',
		triggerLabel,
		ariaLabel,
		class: className = '',
		id,
		onchange,
		leading,
	}: Props = $props()

	let open = $state(false)
	let openUp = $state(false)
	let triggerEl = $state<HTMLButtonElement | null>(null)
	let listEl = $state<HTMLUListElement | null>(null)
	let highlighted = $state(-1)
	let menuStyle = $state('')

	const listId = $derived(id ? `${id}-listbox` : undefined)
	const selected = $derived(
		options.find((o) => String(o.value) === String(value)),
	)
	const displayLabel = $derived(
		triggerLabel ?? selected?.label ?? (value ? String(value) : placeholder),
	)
	const showingPlaceholder = $derived(!selected && !value)

	function firstEnabledIndex(): number {
		return options.findIndex((o) => !o.disabled)
	}

	function lastEnabledIndex(): number {
		for (let i = options.length - 1; i >= 0; i--) {
			if (!options[i]?.disabled) return i
		}
		return -1
	}

	function updateMenuPosition() {
		if (!triggerEl) return
		const rect = triggerEl.getBoundingClientRect()
		const maxH = 240
		const gap = 4
		const spaceBelow = window.innerHeight - rect.bottom - 8
		const spaceAbove = rect.top - 8
		openUp = spaceBelow < 140 && spaceAbove > spaceBelow
		const height = Math.min(
			maxH,
			Math.max(80, openUp ? spaceAbove : spaceBelow),
		)

		const width = Math.round(
			Math.max(rect.width, variant === 'quiet' ? 224 : 0),
		)
		const left = Math.round(
			Math.min(rect.left, Math.max(8, window.innerWidth - width - 8)),
		)
		if (openUp) {
			menuStyle = [
				'position:fixed',
				`left:${left}px`,
				`width:${width}px`,
				`bottom:${Math.round(window.innerHeight - rect.top + gap)}px`,
				`max-height:${Math.round(height)}px`,
			].join(';')
		} else {
			menuStyle = [
				'position:fixed',
				`left:${left}px`,
				`width:${width}px`,
				`top:${Math.round(rect.bottom + gap)}px`,
				`max-height:${Math.round(height)}px`,
			].join(';')
		}
	}

	function openMenu() {
		if (disabled || options.length === 0) return
		// Style must exist before mount — empty style paints in-flow and
		// the first open lands in the wrong place.
		updateMenuPosition()
		open = true
		const current = options.findIndex((o) => o.value === value && !o.disabled)
		highlighted = current >= 0 ? current : firstEnabledIndex()
		requestAnimationFrame(() => {
			updateMenuPosition()
			listEl?.focus({ preventScroll: true })
			scrollHighlightedIntoView()
		})
	}

	function closeMenu(focusTrigger = true) {
		if (!open) return
		open = false
		highlighted = -1
		if (focusTrigger) triggerEl?.focus()
	}

	function selectOption(opt: SelectOption) {
		if (opt.disabled) return
		const next = String(opt.value)
		value = next
		onchange?.(next)
		closeMenu()
	}

	function moveHighlight(delta: number) {
		if (options.length === 0) return
		let i = highlighted
		for (let n = 0; n < options.length; n++) {
			i = (i + delta + options.length) % options.length
			if (!options[i]?.disabled) {
				highlighted = i
				scrollHighlightedIntoView()
				return
			}
		}
	}

	function scrollHighlightedIntoView() {
		if (!listEl || highlighted < 0) return
		const el = listEl.querySelector<HTMLElement>(
			`[data-index="${highlighted}"]`,
		)
		if (!el) return
		const top = el.offsetTop
		const bottom = top + el.offsetHeight
		if (top < listEl.scrollTop) listEl.scrollTop = top
		else if (bottom > listEl.scrollTop + listEl.clientHeight) {
			listEl.scrollTop = bottom - listEl.clientHeight
		}
	}

	function onTriggerClick() {
		if (open) closeMenu()
		else openMenu()
	}

	function onTriggerKeydown(e: KeyboardEvent) {
		if (disabled) return
		if (
			e.key === 'ArrowDown' ||
			e.key === 'ArrowUp' ||
			e.key === 'Enter' ||
			e.key === ' '
		) {
			e.preventDefault()
			if (!open) openMenu()
			else if (e.key === 'ArrowDown') moveHighlight(1)
			else if (e.key === 'ArrowUp') moveHighlight(-1)
		}
	}

	function onListKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault()
			closeMenu()
			return
		}
		if (e.key === 'ArrowDown') {
			e.preventDefault()
			moveHighlight(1)
			return
		}
		if (e.key === 'ArrowUp') {
			e.preventDefault()
			moveHighlight(-1)
			return
		}
		if (e.key === 'Home') {
			e.preventDefault()
			highlighted = firstEnabledIndex()
			scrollHighlightedIntoView()
			return
		}
		if (e.key === 'End') {
			e.preventDefault()
			highlighted = lastEnabledIndex()
			scrollHighlightedIntoView()
			return
		}
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault()
			const opt = options[highlighted]
			if (opt) selectOption(opt)
			return
		}
		if (e.key === 'Tab') {
			closeMenu(false)
		}
	}

	// Outside click + scroll/resize while open
	$effect(() => {
		if (!open) return

		const onPointerDown = (e: PointerEvent) => {
			const t = e.target as Node | null
			if (triggerEl?.contains(t) || listEl?.contains(t)) return
			closeMenu(false)
		}
		const onReposition = () => updateMenuPosition()
		const onScroll = (e: Event) => {
			// Ignore scrolls inside the menu itself
			if (listEl && e.target instanceof Node && listEl.contains(e.target))
				return
			updateMenuPosition()
		}

		document.addEventListener('pointerdown', onPointerDown, true)
		window.addEventListener('resize', onReposition)
		// capture scroll from any ancestor
		window.addEventListener('scroll', onScroll, true)

		return () => {
			document.removeEventListener('pointerdown', onPointerDown, true)
			window.removeEventListener('resize', onReposition)
			window.removeEventListener('scroll', onScroll, true)
		}
	})
</script>

<div
	class={cn(
		variant === 'quiet'
			? 'relative inline-flex min-w-0 max-w-full'
			: 'relative w-full',
		className,
	)}
>
	<button
		bind:this={triggerEl}
		type="button"
		{id}
		class={cn(
			'flex cursor-pointer items-center text-left',
			'transition-[border-color,box-shadow,background-color] duration-150 ease-standard',
			'disabled:cursor-not-allowed disabled:opacity-55',
			variant === 'quiet' &&
				'h-control-sm max-w-full gap-1 rounded-md border border-transparent bg-transparent px-1.5 text-caption text-fg-secondary hover:bg-hover hover:text-fg',
			variant === 'quiet' && open && 'bg-hover text-fg',
			variant === 'field' &&
				'w-full justify-between gap-2 rounded-md border border-line-strong bg-field px-3 text-fg focus-field',
			variant === 'field' &&
				(inputSize === 'sm' ? 'h-control-sm text-label' : 'h-field text-body'),
		)}
		{disabled}
		aria-label={ariaLabel}
		aria-haspopup="listbox"
		aria-expanded={open}
		aria-controls={listId}
		onclick={onTriggerClick}
		onkeydown={onTriggerKeydown}
	>
		{#if leading}
			<span
				class="grid shrink-0 place-items-center text-fg-muted"
				aria-hidden="true"
			>
				{@render leading()}
			</span>
		{/if}
		<span
			class={cn(
				'min-w-0 truncate',
				variant === 'quiet' ? 'max-w-40' : 'flex-1',
				showingPlaceholder && 'text-fg-muted',
			)}
		>
			{displayLabel}
		</span>
		<Icon
			name="chevron-down"
			class={cn(
				'shrink-0 text-fg-muted transition-transform duration-150',
				variant === 'quiet' || inputSize === 'sm' ? 'size-3.5' : 'size-4',
				open && 'rotate-180',
			)}
		/>
	</button>

	{#if open}
		<ul
			bind:this={listEl}
			id={listId}
			class={cn(
				'z-50 m-0 overflow-auto rounded-lg border border-line bg-raised p-1 shadow-md',
				'focus:outline-none',
			)}
			style={menuStyle}
			transition:popover={{ origin: openUp ? 'bottom center' : 'top center' }}
			role="listbox"
			tabindex="-1"
			aria-activedescendant={highlighted >= 0 && id
				? `${id}-opt-${highlighted}`
				: undefined}
			onkeydown={onListKeydown}
		>
			{#each options as opt, i (opt.value)}
				{@const isSelected = String(opt.value) === String(value)}
				{@const isActive = i === highlighted}
				<li
					id={id ? `${id}-opt-${i}` : undefined}
					class="list-none"
					role="option"
					aria-selected={isSelected}
					aria-disabled={opt.disabled || undefined}
					data-index={i}
				>
					<button
						type="button"
						tabindex="-1"
						disabled={opt.disabled}
						class={cn(
							'flex w-full items-center rounded-md px-2.5 text-left transition-colors',
							inputSize === 'sm'
								? 'h-control-sm text-label'
								: 'h-control text-body',
							opt.disabled && 'cursor-not-allowed opacity-45',
							!opt.disabled && isActive && 'bg-hover text-fg',
							!opt.disabled &&
								isSelected &&
								!isActive &&
								'bg-accent-soft text-accent-text',
							!opt.disabled &&
								isSelected &&
								isActive &&
								'bg-accent-soft text-accent-text',
							!opt.disabled && !isSelected && !isActive && 'text-fg',
						)}
						onpointerenter={() => {
							if (!opt.disabled) highlighted = i
						}}
						onclick={() => selectOption(opt)}
					>
						<span class="min-w-0 flex-1 truncate">{opt.label}</span>
						{#if isSelected}
							<Icon name="check" class="ml-2 size-3.5 text-accent-text" />
						{/if}
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</div>
