<script lang="ts">
	import { Icon, type IconName } from '#core/ui'
	import { iconInner } from '#icons'
	import { cn } from '#core/util'
	import type { CatalogIconName, CatalogIconTone } from '../icon-types'
	interface Props {
		name: CatalogIconName
		class?: string
		/** Active / selected accent */
		active?: boolean
		tone?: CatalogIconTone
	}

	let { name, class: className = '', active = false, tone }: Props = $props()

	const resolved = $derived(tone ?? (active ? 'accent' : 'muted'))
	const color = $derived(
		resolved === 'accent'
			? 'text-accent'
			: resolved === 'info'
				? 'text-info'
				: resolved === 'success'
					? 'text-success'
					: 'text-fg-muted',
	)
	/** Open catalog kinds (functions, sequences, …) without a glyph of their own. */
	function glyph(n: string): IconName {
		if (n === 'chevron') return 'chevron-right'
		if (n in iconInner) return n as IconName
		if (n === 'matview') return 'view'
		return 'code'
	}

	const iconName = $derived(glyph(name))
</script>

<Icon name={iconName} class={cn(color, className)} />
