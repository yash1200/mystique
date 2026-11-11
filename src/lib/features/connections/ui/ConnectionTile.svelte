<script lang="ts">
	import type { EngineId } from '#core/contracts'
	import { cn } from '#core/util'
	import { adapterFor, engineLabel } from '#engines'

	interface Props {
		engine: EngineId | string
		/** Connection colour (hex). */
		color: string
		size?: 'sm' | 'lg'
		class?: string
	}

	let { engine, color, size = 'sm', class: className = '' }: Props = $props()

	/** Two letters: the engine mark (Pg, My, Mo), or the start of its name. */
	const mark = $derived.by(() => {
		const m = adapterFor(engine).mark
		if (m.length <= 2) return m
		const name = engineLabel(engine)
		return name.charAt(0).toUpperCase() + name.charAt(1).toLowerCase()
	})
</script>

<span
	class={cn(
		'conn-tile',
		size === 'lg'
			? 'size-14 rounded-xl text-body-lg'
			: 'size-7 rounded-md text-[0.65625rem]',
		className,
	)}
	style:--connection-color={color}
	title={engineLabel(engine)}
	aria-hidden="true"
>
	{mark}
</span>
