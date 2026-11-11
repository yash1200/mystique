<script lang="ts">
	import { cn } from '#core/util'
	export type SegmentedOption = {
		value: string
		label: string
		icon?: string
		/** A 7px status dot before the label (a CSS colour). */
		dot?: string
	}

	interface Props {
		value?: string
		options?: SegmentedOption[]
		label?: string
		/** sm: 24px items for dialog headers. */
		size?: 'sm' | 'md'
		disabled?: boolean
		class?: string
		onchange?: (value: string) => void
	}

	let {
		value = '',
		options = [],
		label,
		size = 'md',
		disabled = false,
		class: className = '',
		onchange,
	}: Props = $props()
</script>

<div
	class={cn(
		'inline-flex shrink-0 gap-0.5 bg-hover p-0.5 shadow-[inset_0_0_0_1px_var(--line)]',
		size === 'sm' ? 'rounded-md' : 'rounded-[9px]',
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
				'inline-flex items-center justify-center gap-1.5 border-none text-label font-medium transition-[color,background-color,box-shadow] duration-(--duration-base)',
				size === 'sm' ? 'h-6 rounded-sm px-2.5' : 'h-7 rounded-[7px] px-3',
				value === opt.value
					? 'bg-surface text-fg shadow-[var(--shadow-1),0_0_0_1px_var(--line-2)]'
					: 'bg-transparent text-fg-secondary hover:text-fg',
			)}
			aria-pressed={value === opt.value}
			{disabled}
			onclick={() => {
				if (disabled) return
				onchange?.(opt.value)
			}}
		>
			{#if opt.dot}
				<span class="size-[7px] rounded-full" style:background={opt.dot} aria-hidden="true"></span>
			{:else if opt.icon}
				<span aria-hidden="true">{opt.icon}</span>
			{/if}
			{opt.label}
		</button>
	{/each}
</div>
