<script lang="ts">
	import { cn } from "$lib/utils/cn";

	interface Props {
		checked?: boolean;
		disabled?: boolean;
		label?: string;
		onchange?: (checked: boolean) => void;
		class?: string;
	}

	let {
		checked = false,
		disabled = false,
		label,
		onchange,
		class: className = "",
	}: Props = $props();
</script>

<label
	class={cn(
		"inline-flex select-none items-center gap-2 text-body text-fg-secondary",
		disabled ? "cursor-default opacity-60" : "cursor-pointer",
		className,
	)}
>
	<button
		type="button"
		role="switch"
		aria-checked={checked}
		aria-label={label}
		{disabled}
		class={cn(
			"relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border transition-colors duration-150",
			checked
				? "border-transparent bg-accent"
				: "border-line bg-input hover:border-line-strong",
		)}
		onclick={() => {
			if (disabled) return;
			onchange?.(!checked);
		}}
	>
		<span
			class={cn(
				"pointer-events-none size-3.5 rounded-full bg-elevated shadow-sm transition-transform duration-150",
				checked ? "translate-x-4" : "translate-x-0.5",
			)}
			aria-hidden="true"
		></span>
	</button>
	{#if label}
		<span>{label}</span>
	{/if}
</label>
