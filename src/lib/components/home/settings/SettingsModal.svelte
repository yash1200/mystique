<script lang="ts">
	import { Icon, IconButton, Modal } from '$lib/components/core'
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
	<Icon name="settings" />
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
				<Icon name="close" />
			</IconButton>
		</div>

		{#if section === 'appearance'}
			<AppearanceSettings />
		{:else}
			<McpSettings />
		{/if}
	</div>
</Modal>
