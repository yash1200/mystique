<script lang="ts">
	import { IconButton, Modal } from '$lib/components/core'
	import { cn } from '$lib/utils/cn'
	import AppearanceSettings from './AppearanceSettings.svelte'
	import McpSettings from './McpSettings.svelte'

	type Section = 'appearance' | 'mcp'

	interface Props {
		buttonSize?: 'sm' | 'md'
	}

	let { buttonSize = 'sm' }: Props = $props()

	let open = $state(false)
	let section = $state<Section>('appearance')

	const sections: { id: Section; label: string; title: string }[] = [
		{ id: 'appearance', label: 'Appearance', title: 'Appearance' },
		{ id: 'mcp', label: 'MCP', title: 'MCP server' },
	]

	const current = $derived(
		sections.find((s) => s.id === section) ?? sections[0]!,
	)

	function toggle() {
		open = !open
		if (open) section = 'appearance'
	}

	function close() {
		open = false
	}
</script>

<IconButton
	label="Settings"
	variant="ghost"
	size={buttonSize}
	active={open}
	onclick={toggle}
	aria-expanded={open}
	aria-haspopup="dialog"
>
	<svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
		<path
			d="M6.6 1.85h2.8l.3 1.45c.42.16.81.38 1.16.66l1.48-.38 1.4 2.42-1.18.9c.07.3.11.62.11 1.1 0 .37-.04.69-.11 1.01l1.18.9-1.4 2.42-1.48-.38a4.6 4.6 0 0 1-1.16.66l-.3 1.45H6.6l-.3-1.45a4.6 4.6 0 0 1-1.16-.66l-1.48.38-1.4-2.42 1.18-.9A4.4 4.4 0 0 1 3.33 8c0-.37.04-.69.11-1.01l-1.18-.9 1.4-2.42 1.48.38c.35-.28.74-.5 1.16-.66l.3-1.45Z"
			stroke="currentColor"
			stroke-width="1.2"
			stroke-linejoin="round"
		/>
		<circle cx="8" cy="8" r="1.65" stroke="currentColor" stroke-width="1.2" />
	</svg>
</IconButton>

<Modal
	{open}
	onclose={close}
	layout="bare"
	size="xl"
	height="fill"
	labelledby="settings-title"
>
	<nav
		class="flex w-40 shrink-0 flex-col gap-0.5 border-r border-line bg-panel p-2"
		aria-label="Settings sections"
	>
		<p
			id="settings-title"
			class="mb-1.5 px-2 pt-1 text-caption font-semibold tracking-wider text-fg-muted uppercase"
		>
			Settings
		</p>
		{#each sections as item (item.id)}
			<button
				type="button"
				class={cn(
					'rounded-md border-none px-2 py-1.5 text-left text-label transition-colors',
					section === item.id
						? 'bg-accent-muted text-accent'
						: 'bg-transparent text-fg-secondary hover:bg-hover hover:text-fg',
				)}
				onclick={() => (section = item.id)}
			>
				{item.label}
			</button>
		{/each}
	</nav>

	<div class="min-h-0 min-w-0 flex-1 overflow-auto p-4">
		<div class="mb-3 flex items-start justify-between gap-2">
			<h2 class="m-0 text-body font-semibold text-fg">{current.title}</h2>
			<IconButton
				label="Close settings"
				variant="ghost"
				size="sm"
				onclick={close}
			>
				<svg
					class="size-3.5"
					viewBox="0 0 16 16"
					fill="none"
					aria-hidden="true"
				>
					<path
						d="M4 4l8 8M12 4l-8 8"
						stroke="currentColor"
						stroke-width="1.4"
						stroke-linecap="round"
					/>
				</svg>
			</IconButton>
		</div>

		{#if section === 'appearance'}
			<AppearanceSettings />
		{:else}
			<McpSettings />
		{/if}
	</div>
</Modal>
