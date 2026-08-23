<script lang="ts">
	import { Icon, Input } from '$lib/components/core'

	interface Props {
		value?: string
		placeholder?: string
		id?: string
		label: string
	}

	let {
		value = $bindable(''),
		placeholder = 'Filter…',
		id = 'explorer-filter',
		label,
	}: Props = $props()

	const hasValue = $derived(value.length > 0)

	function clear() {
		value = ''
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key !== 'Escape' || !value) return
		e.stopPropagation()
		clear()
	}
</script>

<div class="relative">
	<label class="sr-only" for={id}>{label}</label>
	<Icon
		name="search"
		class="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-fg-muted"
	/>
	<Input
		{id}
		type="search"
		{placeholder}
		bind:value
		inputSize="sm"
		autocomplete="off"
		inset={hasValue ? 'both' : 'start'}
		class="[appearance:textfield] [&::-webkit-search-cancel-button]:hidden [&::-webkit-search-decoration]:hidden"
		{onkeydown}
	/>
	{#if hasValue}
		<button
			type="button"
			class="absolute top-1/2 right-0.5 grid size-6 -translate-y-1/2 place-items-center rounded-sm border-none bg-transparent text-fg-muted transition-colors duration-150 hover:bg-hover hover:text-fg focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1 enabled:active:scale-[0.97]"
			aria-label="Clear filter"
			onclick={clear}
		>
			<Icon name="close" class="size-3" />
		</button>
	{/if}
</div>
