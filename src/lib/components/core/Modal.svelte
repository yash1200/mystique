<script lang="ts">
	import { cn } from '$lib/utils/cn'
	import { modalPanel, overlayFade } from '$lib/utils/motion'
	import type { Snippet } from 'svelte'
	import IconButton from './IconButton.svelte'

	type Size = 'sm' | 'md' | 'lg' | 'xl'
	type Layout = 'inset' | 'chrome' | 'bare'
	type Height = 'auto' | 'fill' | 'sm' | 'md' | 'lg'

	interface Props {
		open: boolean
		onclose: () => void
		title?: string
		description?: string
		/** Overrides the generated title id for `aria-labelledby`. */
		labelledby?: string
		size?: Size
		/** inset: padded confirm/form. chrome: header/body/footer. bare: shell only. */
		layout?: Layout
		height?: Height
		/** X button in the chrome header. Default: chrome + (title or header). */
		closeButton?: boolean
		/** Escape and backdrop dismiss. */
		dismissible?: boolean
		class?: string
		style?: string
		header?: Snippet
		children?: Snippet
		footer?: Snippet
		onkeydown?: (e: KeyboardEvent) => void
	}

	let {
		open,
		onclose,
		title,
		description,
		labelledby,
		size = 'sm',
		layout = 'inset',
		height = 'auto',
		closeButton,
		dismissible = true,
		class: className = '',
		style,
		header,
		children,
		footer,
		onkeydown,
	}: Props = $props()

	const uid = $props.id()
	const titleId = $derived(labelledby ?? (title ? `${uid}-title` : undefined))
	const showClose = $derived(
		closeButton ?? (layout === 'chrome' && !!(title || header)),
	)
	const showHeader = $derived(
		layout === 'chrome' && !!(title || header || showClose),
	)

	const sizes: Record<Size, string> = {
		sm: 'max-w-md',
		md: 'max-w-lg',
		lg: 'max-w-panel-lg',
		xl: 'max-w-2xl',
	}

	const heights: Record<Height, string> = {
		auto: '',
		fill: 'h-modal',
		sm: 'max-h-modal-sm',
		md: 'max-h-modal-md',
		lg: 'max-h-modal-lg',
	}

	function onWindowKey(e: KeyboardEvent) {
		if (!open || !dismissible) return
		if (e.key === 'Escape') {
			e.preventDefault()
			onclose()
		}
	}

	function onBackdrop() {
		if (dismissible) onclose()
	}
</script>

<svelte:window onkeydown={onWindowKey} />

{#if open}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center p-4"
		role="presentation"
		{onkeydown}
	>
		<button
			type="button"
			class="absolute inset-0 border-none bg-black/45"
			aria-label="Dismiss"
			transition:overlayFade|global
			onclick={onBackdrop}
			disabled={!dismissible}
		></button>
		<div
			class={cn(
				'relative z-10 w-full rounded-lg border border-line bg-elevated shadow-lg',
				sizes[size],
				heights[height],
				layout === 'inset' && 'p-4',
				layout === 'chrome' && 'flex flex-col overflow-hidden',
				layout === 'bare' && 'overflow-hidden',
				layout === 'bare' && height !== 'auto' && 'flex',
				layout === 'inset' &&
					height !== 'auto' &&
					'flex flex-col overflow-hidden',
				className,
			)}
			{style}
			transition:modalPanel|global
			role="dialog"
			aria-modal="true"
			aria-labelledby={titleId}
		>
			{#if layout === 'bare'}
				{@render children?.()}
			{:else if layout === 'chrome'}
				{#if showHeader}
					<header
						class="flex shrink-0 items-start justify-between gap-3 border-b border-line px-4 py-3"
					>
						<div class="min-w-0 flex-1">
							{#if header}
								{@render header()}
							{:else if title}
								<h2 id={titleId} class="m-0 text-body font-semibold text-fg">
									{title}
								</h2>
								{#if description}
									<p class="mt-1 mb-0 text-label leading-relaxed text-fg-muted">
										{description}
									</p>
								{/if}
							{/if}
						</div>
						{#if showClose}
							<IconButton
								label="Close"
								variant="ghost"
								size="sm"
								onclick={onclose}
							>
								<span class="text-body-lg leading-none" aria-hidden="true"
									>×</span
								>
							</IconButton>
						{/if}
					</header>
				{/if}
				<div
					class={cn(
						'min-h-0',
						height !== 'auto' && 'flex flex-1 flex-col overflow-hidden',
					)}
				>
					{@render children?.()}
				</div>
				{#if footer}
					<footer
						class="flex shrink-0 items-center justify-end gap-2 border-t border-line px-4 py-2.5"
					>
						{@render footer()}
					</footer>
				{/if}
			{:else}
				{#if title}
					<h2 id={titleId} class="m-0 text-body font-semibold text-fg">
						{title}
					</h2>
				{/if}
				{#if description}
					<p
						class={cn(
							'mb-0 text-label leading-relaxed text-fg-secondary',
							title ? 'mt-2' : 'mt-0',
						)}
					>
						{description}
					</p>
				{/if}
				{@render children?.()}
				{#if footer}
					<div class="mt-4 flex flex-wrap items-center justify-end gap-2">
						{@render footer()}
					</div>
				{/if}
			{/if}
		</div>
	</div>
{/if}
