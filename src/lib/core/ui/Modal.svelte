<script lang="ts">
	import { cn, modalPanel, overlayFade } from '#core/util'
	import type { IconName } from '#icons'
	import type { Snippet } from 'svelte'
	import { pushDialog } from './dialog-stack'
	import Icon from './Icon.svelte'
	import IconButton from './IconButton.svelte'

	type Size = 'sm' | 'md' | 'lg' | 'xl'
	type Layout = 'inset' | 'chrome' | 'bare'
	type Height = 'auto' | 'fill' | 'sm' | 'md' | 'lg'
	type Tone = 'neutral' | 'accent' | 'warning' | 'danger'

	/**
	 * One surface for every dialog: raised, 16px radius, a header (optional
	 * icon badge, title, description), the body, and a footer bar with the
	 * actions on the right and an optional note on the left.
	 */
	interface Props {
		open: boolean
		onclose: () => void
		title?: string
		/** Rich title (mono names…); replaces `title` text, keeps its id. */
		titleContent?: Snippet
		/** Beside the title: a PROD badge, a step indicator. */
		titleAside?: Snippet
		description?: string
		/** Leading glyph in a 36px badge. */
		icon?: IconName
		tone?: Tone
		/** Left side of the footer bar: what the action touches. */
		note?: string
		/** Overrides the generated title id for `aria-labelledby`. */
		labelledby?: string
		size?: Size
		/** inset: header + body + footer. chrome: the same with a divided header and a scrolling body. bare: shell only. */
		layout?: Layout
		height?: Height
		/** X button in the header. Default: chrome layout only. */
		closeButton?: boolean
		/** Escape and backdrop dismiss. */
		dismissible?: boolean
		/** `alertdialog` for a destructive confirm. */
		role?: 'dialog' | 'alertdialog'
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
		titleContent,
		titleAside,
		description,
		icon,
		tone = 'neutral',
		note,
		labelledby,
		size = 'sm',
		layout = 'inset',
		height = 'auto',
		closeButton,
		dismissible = true,
		role = 'dialog',
		class: className = '',
		style,
		header,
		children,
		footer,
		onkeydown,
	}: Props = $props()

	const uid = $props.id()
	const titleId = $derived(
		labelledby ?? (title || titleContent ? `${uid}-title` : undefined),
	)
	const hasHead = $derived(!!(title || titleContent || header))
	const showClose = $derived(closeButton ?? (layout === 'chrome' && hasHead))
	const sized = $derived(height !== 'auto')

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

	const tones: Record<Tone, string> = {
		neutral: 'bg-hover text-fg-secondary',
		accent: 'bg-accent-soft text-accent-text',
		warning: 'bg-warn-soft text-warning',
		danger: 'bg-danger-soft text-danger',
	}

	$effect(() => {
		if (!open) return
		return pushDialog({ close: () => onclose(), dismissible: () => dismissible })
	})

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

{#snippet head()}
	<header
		class={cn(
			'flex shrink-0 items-start gap-3 px-5',
			layout === 'chrome' ? 'border-b border-line py-4' : 'pt-[1.125rem] pb-3.5',
		)}
	>
		{#if icon}
			<span
				class={cn(
					'grid size-9 shrink-0 place-items-center rounded-[10px] [&_svg]:size-4',
					tones[tone],
				)}
				aria-hidden="true"
			>
				<Icon name={icon} />
			</span>
		{/if}
		<div class={cn('flex min-w-0 flex-1 flex-col gap-1', icon && 'pt-px')}>
			{#if header}
				{@render header()}
			{:else}
				<div class="flex min-w-0 flex-wrap items-center gap-2">
					<h2 id={titleId} class="m-0 min-w-0 text-heading font-semibold tracking-[-0.01em] text-fg">
						{#if titleContent}{@render titleContent()}{:else}{title}{/if}
					</h2>
					{@render titleAside?.()}
				</div>
				{#if description}
					<p class="m-0 text-body text-pretty text-fg-secondary">{description}</p>
				{/if}
			{/if}
		</div>
		{#if showClose}
			<IconButton label="Close" variant="quiet" size="sm" class="-mt-0.5 -mr-1" onclick={onclose}>
				<Icon name="close" />
			</IconButton>
		{/if}
	</header>
{/snippet}

{#if open}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center p-4"
		role="presentation"
		{onkeydown}
	>
		<button
			type="button"
			class="absolute inset-0 border-none bg-scrim"
			aria-label="Dismiss"
			transition:overlayFade|global
			onclick={onBackdrop}
			disabled={!dismissible}
		></button>
		<div
			class={cn(
				'relative z-10 w-full overflow-hidden rounded-xl border border-line bg-raised shadow-lg',
				sizes[size],
				heights[height],
				layout !== 'bare' && 'flex flex-col',
				layout === 'bare' && sized && 'flex',
				className,
			)}
			{style}
			transition:modalPanel|global
			{role}
			aria-modal="true"
			aria-labelledby={titleId}
		>
			{#if layout === 'bare'}
				{@render children?.()}
			{:else}
				{#if hasHead}
					{@render head()}
				{/if}
				{#if children}
					<div
						class={cn(
							layout === 'chrome'
								? 'min-h-0'
								: 'flex flex-col gap-4 px-5 pb-[1.125rem]',
							layout === 'inset' && !hasHead && 'pt-5',
							sized && 'min-h-0 flex-1 overflow-auto',
							sized && layout === 'chrome' && 'flex flex-col',
						)}
					>
						{@render children()}
					</div>
				{/if}
				{#if footer}
					<footer
						class="flex shrink-0 flex-wrap items-center gap-2 border-t border-line bg-surface px-4 py-3"
					>
						<span class="min-w-0 flex-1 text-label text-fg-muted">{note ?? ''}</span>
						{@render footer()}
					</footer>
				{/if}
			{/if}
		</div>
	</div>
{/if}
