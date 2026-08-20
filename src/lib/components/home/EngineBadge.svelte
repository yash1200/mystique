<script lang="ts">
	import { Badge } from '$lib/components/core'
	import type { EngineId } from '$lib/types/connection'
	import { engineLabel } from '$lib/types/connection'

	interface Props {
		engine: EngineId
		size?: 'sm' | 'md'
	}

	let { engine, size = 'sm' }: Props = $props()

	const mark = $derived(
		(
			{
				postgres: 'Pg',
				sqlite: 'SQL',
				mongodb: 'Mo',
				mysql: 'My',
			} as Record<string, string>
		)[String(engine)] ?? String(engine).slice(0, 2).toUpperCase(),
	)
</script>

<span title={engineLabel(engine)} aria-label={engineLabel(engine)}>
	<Badge {size}>{mark}</Badge>
</span>
