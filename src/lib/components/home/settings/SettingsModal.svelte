<script lang="ts">
	import { Icon, IconButton, Modal } from '$lib/components/core'
	import {
		appSettingsUi,
		type SettingsSection,
	} from '$lib/stores/app-settings.svelte'
	import { cn } from '$lib/utils/cn'
	import AiSettings from './AiSettings.svelte'
	import AppearanceSettings from './AppearanceSettings.svelte'
	import McpSettings from './McpSettings.svelte'

	interface Props {
		buttonSize?: 'sm' | 'md'
	}

	let { buttonSize = 'sm' }: Props = $props()

	const sections: { id: SettingsSection; label: string; title: string }[] = [
		{ id: 'appearance', label: 'Appearance', title: 'Appearance' },
		{ id: 'ai', label: 'AI', title: 'AI agent' },
		{ id: 'mcp', label: 'MCP', title: 'MCP server' },
	]

	const current = $derived(
		sections.find((s) => s.id === appSettingsUi.section) ?? sections[0]!,
	)

	function toggle() {
		if (appSettingsUi.open) {
			appSettingsUi.close()
			return
		}
		appSettingsUi.show('appearance')
	}

	function close() {
		appSettingsUi.close()
	}
</script>

<IconButton
	label="Settings"
	variant="ghost"
	size={buttonSize}
	active={appSettingsUi.open}
	onclick={toggle}
	aria-expanded={appSettingsUi.open}
	aria-haspopup="dialog"
>
	<Icon name="settings" />
</IconButton>

<Modal
	open={appSettingsUi.open}
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
					appSettingsUi.section === item.id
						? 'bg-accent-muted text-accent'
						: 'bg-transparent text-fg-secondary hover:bg-hover hover:text-fg',
				)}
				onclick={() => (appSettingsUi.section = item.id)}
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

		{#if appSettingsUi.section === 'appearance'}
			<AppearanceSettings />
		{:else if appSettingsUi.section === 'ai'}
			<AiSettings />
		{:else}
			<McpSettings />
		{/if}
	</div>
</Modal>
