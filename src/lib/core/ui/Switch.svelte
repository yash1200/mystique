<script lang="ts">
	import { cn } from '#core/util'

	interface Props {
		checked?: boolean
		disabled?: boolean
		label?: string
		/** Accessible name when the visible label lives outside the switch. */
		ariaLabel?: string
		description?: string
		onchange?: (checked: boolean) => void
		class?: string
	}

	let {
		checked = false,
		disabled = false,
		label,
		ariaLabel,
		description,
		onchange,
		class: className = '',
	}: Props = $props()
</script>

<div
	class={cn(
		label ? 'flex w-full min-w-0 flex-col gap-1' : 'inline-flex',
		disabled && 'opacity-60',
		className,
	)}
>
	<label
		class={cn(
			'flex select-none items-center gap-3',
			label ? 'w-full justify-between' : 'inline-flex',
			disabled ? 'cursor-default' : 'cursor-pointer',
		)}
	>
		{#if label}
			<span class="min-w-0 text-label font-medium text-fg-secondary"
				>{label}</span
			>
		{/if}
		<button
			type="button"
			role="switch"
			aria-checked={checked}
			aria-label={ariaLabel ?? label}
			{disabled}
			class={cn(
				'relative inline-flex h-5 w-[34px] shrink-0 items-center rounded-full border-none p-0 transition-colors duration-200',
				checked
					? 'bg-accent'
					: 'bg-pressed shadow-[inset_0_0_0_1px_var(--line-2)]',
			)}
			onclick={() => {
				if (disabled) return
				onchange?.(!checked)
			}}
		>
			<span
				class={cn(
					'pointer-events-none size-4 rounded-full bg-white shadow-[0_1px_3px_rgb(0_0_0/0.3)] transition-transform duration-(--duration-spring) ease-spring',
					checked ? 'translate-x-[16px]' : 'translate-x-0.5',
				)}
				aria-hidden="true"
			></span>
		</button>
	</label>
	{#if description}
		<p class="m-0 text-caption text-fg-muted">{description}</p>
	{/if}
</div>
