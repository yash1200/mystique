<script lang="ts">
	import { Icon, type IconName } from '$core/ui'
	import { cn } from '$core/util'
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
	const iconName = $derived(
		(name === 'chevron' ? 'chevron-right' : name) as IconName,
	)
</script>

<Icon name={iconName} class={cn(color, className)} />
