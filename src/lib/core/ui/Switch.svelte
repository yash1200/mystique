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
				'relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border transition-colors duration-150',
				checked
					? 'border-transparent bg-accent'
					: 'border-line bg-input hover:border-line-strong',
			)}
			onclick={() => {
				if (disabled) return
				onchange?.(!checked)
			}}
		>
			<span
				class={cn(
					'pointer-events-none size-3.5 rounded-full bg-elevated shadow-sm transition-transform duration-150',
					checked ? 'translate-x-4' : 'translate-x-0.5',
				)}
				aria-hidden="true"
			></span>
		</button>
	</label>
	{#if description}
		<p class="m-0 text-caption text-fg-muted">{description}</p>
	{/if}
</div>
