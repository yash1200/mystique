<script lang="ts">
	import { cn } from '$core/util'
	export type SegmentedOption = {
		value: string
		label: string
		icon?: string
	}

	interface Props {
		value?: string
		options?: SegmentedOption[]
		label?: string
		disabled?: boolean
		class?: string
		onchange?: (value: string) => void
	}

	let {
		value = '',
		options = [],
		label,
		disabled = false,
		class: className = '',
		onchange,
	}: Props = $props()
</script>

<div
	class={cn(
		'flex gap-0.5 rounded-md border border-line bg-input p-0.5',
		disabled && 'opacity-60',
		className,
	)}
	role="group"
	aria-label={label}
>
	{#each options as opt (opt.value)}
		<button
			type="button"
			class={cn(
				'inline-flex items-center justify-center gap-1 rounded-sm border-none px-2 py-1 text-caption font-medium transition-colors duration-150',
				'focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1',
				value === opt.value
					? 'bg-elevated text-fg shadow-sm'
					: 'bg-transparent text-fg-muted hover:bg-hover hover:text-fg-secondary',
			)}
			aria-pressed={value === opt.value}
			{disabled}
			onclick={() => {
				if (disabled) return
				onchange?.(opt.value)
			}}
		>
			{#if opt.icon}
				<span aria-hidden="true">{opt.icon}</span>
			{/if}
			{opt.label}
		</button>
	{/each}
</div>
