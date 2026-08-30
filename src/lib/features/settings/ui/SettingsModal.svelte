<script lang="ts">
	import { Icon, IconButton, Modal, type IconName } from '$core/ui'
	import { cn } from '$core/util'
	import { appSettingsUi } from '../model.svelte'
	import type { SettingsSection } from '../prefs'
	import AiSettings from './AiSettings.svelte'
	import AppearanceSettings from './AppearanceSettings.svelte'
	import McpSettings from './McpSettings.svelte'
	import WorkspaceSettings from './WorkspaceSettings.svelte'

	interface Props {
		buttonSize?: 'sm' | 'md'
		buttonVariant?: 'ghost' | 'quiet'
	}

	let { buttonSize = 'sm', buttonVariant = 'ghost' }: Props = $props()

	const sections: {
		id: SettingsSection
		label: string
		title: string
		icon: IconName
		description?: string
	}[] = [
		{
			id: 'appearance',
			label: 'Appearance',
			title: 'Appearance',
			icon: 'grid',
			description: 'Applies to the whole app.',
		},
		{
			id: 'workspace',
			label: 'Workspace',
			title: 'Workspace',
			icon: 'table',
			description: 'Applies to the workspace.',
		},
		{
			id: 'ai',
			label: 'AI',
			title: 'AI agent',
			icon: 'chat',
			description: 'Draft queries from the open connection.',
		},
		{
			id: 'mcp',
			label: 'MCP',
			title: 'MCP server',
			icon: 'code',
			description: 'Let other AI tools query your databases while Mystique is open.',
		},
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
	variant={buttonVariant}
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
		class="flex w-48 shrink-0 flex-col border-r border-line bg-panel px-2 py-3"
		aria-label="Settings sections"
	>
		<p
			id="settings-title"
			class="mb-3 px-2 text-body-lg font-semibold tracking-tight text-fg"
		>
			Settings
		</p>
		<div class="flex flex-col gap-0.5">
			{#each sections as item (item.id)}
				{@const active = appSettingsUi.section === item.id}
				<button
					type="button"
					class={cn(
						'flex items-center gap-2 rounded-md border-none px-2 py-2 text-left text-label font-medium transition-colors duration-150',
						active
							? 'bg-accent-muted text-accent'
							: 'bg-transparent text-fg-secondary hover:bg-hover hover:text-fg',
					)}
					aria-current={active ? 'page' : undefined}
					onclick={() => (appSettingsUi.section = item.id)}
				>
					<Icon
						name={item.icon}
						class={cn('size-3.5', active ? 'text-accent' : 'text-fg-muted')}
					/>
					{item.label}
				</button>
			{/each}
		</div>
	</nav>

	<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-elevated">
		<header
			class="flex shrink-0 items-start justify-between gap-3 border-b border-line px-5 py-3"
		>
			<div class="min-w-0">
				<h2
					id="settings-pane-title"
					class="m-0 text-title font-semibold tracking-tight text-fg"
				>
					{current.title}
				</h2>
				{#if current.description}
					<p
						class="mt-1 mb-0 max-w-prose-ui text-label leading-relaxed text-fg-secondary"
					>
						{current.description}
					</p>
				{/if}
			</div>
			<IconButton
				label="Close settings"
				variant="quiet"
				size="sm"
				onclick={close}
			>
				<Icon name="close" />
			</IconButton>
		</header>

		<div class="min-h-0 flex-1 overflow-auto px-5 py-4">
			{#if appSettingsUi.section === 'appearance'}
				<AppearanceSettings />
			{:else if appSettingsUi.section === 'workspace'}
				<WorkspaceSettings />
			{:else if appSettingsUi.section === 'ai'}
				<AiSettings />
			{:else}
				<McpSettings />
			{/if}
		</div>
	</div>
</Modal>
