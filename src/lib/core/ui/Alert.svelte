<script lang="ts">
	import { cn, fadeRise } from '#core/util'
	import type { IconName } from '#icons'
	import type { Snippet } from 'svelte'
	import Icon from './Icon.svelte'

	type Tone = 'info' | 'success' | 'warning' | 'danger'

	interface Props {
		tone?: Tone
		/** Leading tone glyph. */
		icon?: boolean
		/** Page banner (13px, roomier) rather than an inline form note. */
		banner?: boolean
		class?: string
		children?: Snippet
	}

	let {
		tone = 'info',
		icon = true,
		banner = false,
		class: className = '',
		children,
	}: Props = $props()

	const tones: Record<Tone, { box: string; glyph: IconName; ink: string }> = {
		info: { box: 'bg-info-soft text-fg [--tint:var(--info)]', glyph: 'info', ink: 'text-info' },
		success: {
			box: 'bg-accent-soft text-accent-text [--tint:var(--accent)]',
			glyph: 'check',
			ink: 'text-accent-text',
		},
		warning: {
			box: 'bg-warn-soft text-fg [--tint:var(--warn)]',
			glyph: 'shield-alert',
			ink: 'text-warning',
		},
		danger: {
			box: 'bg-danger-soft text-danger [--tint:var(--danger)]',
			glyph: 'alert-circle',
			ink: 'text-danger',
		},
	}

	const t = $derived(tones[tone])
</script>

<div
	class={cn(
		'm-0 flex items-start gap-2.5 rounded-lg ring-tint',
		banner ? 'gap-3 px-4 py-3 text-body' : 'px-3.5 py-2.5 text-label',
		t.box,
		className,
	)}
	role={tone === 'danger' ? 'alert' : 'status'}
	transition:fadeRise|global
>
	{#if icon}
		<span class={cn('flex', banner ? 'mt-0.5 [&_svg]:size-4' : 'mt-px', t.ink)}>
			<Icon name={t.glyph} />
		</span>
	{/if}
	<div class="min-w-0 flex-1 text-pretty">{@render children?.()}</div>
</div>
