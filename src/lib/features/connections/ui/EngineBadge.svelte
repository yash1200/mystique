<script lang="ts">
	import type { EngineId } from '$core/contracts'
	import { Badge } from '$core/ui'
	import { adapterFor, engineLabel } from '$engines'
	interface Props {
		engine: EngineId
		size?: 'sm' | 'md'
	}

	let { engine, size = 'sm' }: Props = $props()

	const mark = $derived(adapterFor(engine).mark)
</script>

<span title={engineLabel(engine)} aria-label={engineLabel(engine)}>
	<Badge {size}>{mark}</Badge>
</span>
