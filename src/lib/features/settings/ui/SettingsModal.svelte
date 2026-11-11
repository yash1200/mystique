<script lang="ts" module>
	import { lazy } from '#core/ui'

	// The AI pane brings the agent feature with it; neither is needed until
	// someone opens these sections.
	const AiSettings = lazy(() => import('./AiSettings.svelte'))
	const McpSettings = lazy(() => import('./McpSettings.svelte'))
</script>

<script lang="ts">
	import {
		Icon,
		IconButton,
		Lazy,
		Mascot,
		Modal,
		type IconName,
		type MascotMood,
	} from '#core/ui'
	import { cn } from '#core/util'
	import { appShortcutsUi } from '#features/shortcuts'
	import { appPrefs } from '#lib/app'
	import { getMcpStatus } from '../api'
	import { appSettingsUi } from '../model.svelte'
	import type { SettingsSection } from '../prefs'
	import AboutSettings from './AboutSettings.svelte'
	import AppearanceSettings from './AppearanceSettings.svelte'
	import PasswordsSettings from './PasswordsSettings.svelte'
	import WorkspaceSettings from './WorkspaceSettings.svelte'

	interface Props {
		buttonSize?: 'sm' | 'md'
		buttonVariant?: 'ghost' | 'quiet'
	}

	let { buttonSize = 'sm', buttonVariant = 'ghost' }: Props = $props()

	const TITLE_ID = 'settings-title'
	const AWAKE_MS = 2200

	const sections: {
		id: SettingsSection
		label: string
		icon: IconName
		description: string
	}[] = [
		{ id: 'appearance', label: 'Appearance', icon: 'contrast', description: 'Applies to every window.' },
		{ id: 'workspace', label: 'Workspace', icon: 'sidebar', description: 'How tables and results behave.' },
		{ id: 'passwords', label: 'Passwords', icon: 'lock', description: 'How Mystique protects the passwords you save.' },
		{ id: 'ai', label: 'AI agent', icon: 'sparkle', description: 'Draft queries from the open connection.' },
		{
			id: 'mcp',
			label: 'MCP server',
			icon: 'plug',
			description: 'Let other AI tools query your databases while Mystique is open.',
		},
		{ id: 'about', label: 'About', icon: 'info', description: 'Version and diagnostics for bug reports.' },
	]

	/** The MCP server is listening (the dot beside its section). */
	let mcpLive = $state(false)
	let awake = $state(false)
	let awakeTimer: ReturnType<typeof setTimeout> | undefined

	const current = $derived(
		sections.find((s) => s.id === appSettingsUi.section) ?? sections[0]!,
	)

	/** Night themes put the dog to sleep; a pet or a 150% scale wakes it. */
	const dog = $derived.by((): { mood: MascotMood; line: string } => {
		if (awake) return { mood: 'happy', line: 'Five more minutes…' }
		if (appSettingsUi.scale >= 1.5) return { mood: 'happy', line: 'Big dog energy.' }
		return appPrefs.resolvedTheme === 'dark'
			? { mood: 'sleep', line: 'Shh. Night mode means nap time.' }
			: { mood: 'idle', line: 'Daylight! Ready to fetch.' }
	})

	$effect(() => {
		if (!appSettingsUi.open) return
		// Read again on every section change, so the dot follows a toggle on the MCP pane.
		void appSettingsUi.section
		let gone = false
		getMcpStatus()
			.then((status) => {
				if (!gone) mcpLive = status.running
			})
			.catch(() => {
				/* not in the desktop shell */
			})
		return () => {
			gone = true
		}
	})

	$effect(() => () => clearTimeout(awakeTimer))

	function wake() {
		awake = true
		clearTimeout(awakeTimer)
		awakeTimer = setTimeout(() => (awake = false), AWAKE_MS)
	}

	function toggle() {
		if (appSettingsUi.open) appSettingsUi.close()
		else appSettingsUi.show('appearance')
	}

	function close() {
		appSettingsUi.close()
	}

	function openShortcuts() {
		appSettingsUi.close()
		appShortcutsUi.show()
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
	<Icon name="sliders" />
</IconButton>

{#snippet navItem(icon: IconName, label: string, on: boolean, onclick: () => void, trailing?: 'live' | 'out')}
	<button
		type="button"
		class={cn(
			'flex h-[2.125rem] w-full items-center gap-2.5 rounded-md border-none px-2.5 text-left font-medium transition-colors duration-150',
			on
				? 'bg-hover text-fg shadow-[inset_0_0_0_1px_var(--line)]'
				: 'bg-transparent text-fg-secondary hover:bg-hover hover:text-fg',
		)}
		aria-current={on ? 'page' : undefined}
		{onclick}
	>
		<span class={cn('flex [&_svg]:size-4', on ? 'text-accent-text' : 'text-fg-muted')}>
			<Icon name={icon} />
		</span>
		<span class="min-w-0 flex-1 truncate">{label}</span>
		{#if trailing === 'live'}
			<span class="size-1.5 rounded-full dot-live" role="img" aria-label="Running"></span>
		{:else if trailing === 'out'}
			<span class="flex text-fg-muted [&_svg]:size-3.5"><Icon name="external" /></span>
		{/if}
	</button>
{/snippet}

<Modal
	open={appSettingsUi.open}
	onclose={close}
	layout="bare"
	size="lg"
	height="fill"
	labelledby={TITLE_ID}
>
	<div class="flex h-full min-h-0 w-full flex-wrap">
		<nav
			class="flex max-w-full flex-[1_1_13.75rem] flex-col gap-0.5 border-r border-line bg-surface px-2.5 pt-[1.125rem] pb-3.5"
			aria-label="Settings sections"
		>
			<h2 id={TITLE_ID} class="m-0 px-2.5 pb-3 text-heading font-semibold">Settings</h2>
			{#each sections as item (item.id)}
				{@render navItem(
					item.icon,
					item.label,
					appSettingsUi.section === item.id,
					() => (appSettingsUi.section = item.id),
					item.id === 'mcp' && mcpLive ? 'live' : undefined,
				)}
			{/each}
			<div class="mx-1.5 my-2.5 h-px bg-line" role="separator"></div>
			{@render navItem('keyboard', 'Keyboard shortcuts', false, openShortcuts, 'out')}
			<div class="flex-1"></div>
			<div class="flex items-center gap-2.5 rounded-[10px] bg-hover p-2.5">
				<button
					type="button"
					class="flex rounded-md border-none bg-transparent p-0"
					aria-label="Wake Mystique"
					onclick={wake}
				>
					<Mascot mood={dog.mood} size={34} playful />
				</button>
				<span class="text-label leading-snug text-fg-muted" aria-live="polite">{dog.line}</span>
			</div>
		</nav>

		<section class="flex h-full min-w-0 flex-[999_1_27.5rem] flex-col" aria-labelledby="settings-pane-title">
			<header class="flex shrink-0 items-start gap-3 border-b border-line px-6 pt-5 pb-4">
				<div class="flex min-w-0 flex-1 flex-col gap-0.5">
					<h3 id="settings-pane-title" class="m-0 text-title leading-tight font-semibold tracking-[-0.015em]">
						{current.label}
					</h3>
					<p class="m-0 text-fg-muted">{current.description}</p>
				</div>
				<IconButton label="Close settings" variant="quiet" size="sm" onclick={close}>
					<Icon name="close" />
				</IconButton>
			</header>

			<div class="min-h-0 flex-1 overflow-auto px-6 pt-5 pb-7">
				{#key appSettingsUi.section}
					<div class="animate-rise">
						{#if appSettingsUi.section === 'appearance'}
							<AppearanceSettings />
						{:else if appSettingsUi.section === 'workspace'}
							<WorkspaceSettings />
						{:else if appSettingsUi.section === 'passwords'}
							<PasswordsSettings />
						{:else if appSettingsUi.section === 'ai'}
							<Lazy component={AiSettings} />
						{:else if appSettingsUi.section === 'about'}
							<AboutSettings />
						{:else}
							<Lazy component={McpSettings} />
						{/if}
					</div>
				{/key}
			</div>
		</section>
	</div>
</Modal>
