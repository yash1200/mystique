<script lang="ts" module>
	import { lazy } from '#core/ui'

	// Only the workspace has changes a reset can lose, so this comes with the
	// workspace's code and never weighs on the home page.
	const ResetConfirmModal = lazy(() =>
		import('#features/workspace/shell').then((m) => ({
			default: m.ResetConfirmModal,
		})),
	)
</script>

<script lang="ts">
	import { commands, mountCommands } from '#core/commands'
	import { confirmQuit, listenQuitRequests } from '#core/ipc'
	import { Button, Lazy, Modal } from '#core/ui'
	import {
		applyPreferredUiScale,
		applyUiScale,
		blockHistoryBackspace,
		setAppTitle,
		writeScalePreference,
	} from '#core/util'
	import { ApprovalDock } from '#features/approvals'
	import { PasswordPromptModal, SshHostKeyModal } from '#features/connections'
	import { appSettingsUi, type SettingsSection } from '#features/settings'
	import { appShortcutsUi, KeyboardShortcutsModal } from '#features/shortcuts'
	import { ToastContext } from '#features/toast'
	import {
		QuickOpen,
		quickOpen,
		registerQuickOpenCommands,
	} from '#features/workspace/quick-open'
	import { resetGuard } from '#features/workspace/session'
	import { appPrefs, cmd, KEYMAP } from '#lib/app'
	import { onDestroy, onMount } from 'svelte'
	import '../app.css'

	let { children } = $props()

	/** Transfers still running when a quit or window close was held. */
	let quitHeld = $state<number | null>(null)
	let quitting = $state(false)

	async function quitAnyway() {
		quitting = true
		try {
			await confirmQuit()
		} finally {
			quitting = false
		}
	}

	// The keymap is known before anything mounts, so the shortcuts view can
	// list every command.
	commands.declare(KEYMAP)

	function zoomBy(delta: number) {
		const cur = Number(document.documentElement.dataset.uiScale) || 1
		const next = Math.min(
			2.5,
			Math.max(1, Math.round((cur + delta) * 100) / 100),
		)
		writeScalePreference(next)
		applyUiScale(next)
	}

	function zoomReset() {
		writeScalePreference(1)
		applyUiScale(1)
	}

	// App-wide handlers go in before any page mounts, so a page's own handler
	// for the same chord (the diagram's zoom) is newer and wins.
	const unregister = commands.register([
		{
			...cmd.settings,
			run: ({ args }) =>
				appSettingsUi.show(args?.section as SettingsSection | undefined),
		},
		{ ...cmd.shortcuts, run: () => appShortcutsUi.show() },
		{ ...cmd.reset, run: () => resetGuard.request() },
		{ ...cmd.zoomIn, run: () => zoomBy(0.1) },
		{ ...cmd.zoomOut, run: () => zoomBy(-0.1) },
		{ ...cmd.zoomReset, run: zoomReset },
	])
	const unregisterQuickOpen = registerQuickOpenCommands()
	onDestroy(() => {
		unregister()
		unregisterQuickOpen()
	})

	onMount(() => {
		appPrefs.apply()
		const unwatchScheme = appPrefs.watchSystemTheme()
		applyPreferredUiScale()
		void setAppTitle()

		const unmountCommands = mountCommands()
		window.addEventListener('keydown', blockHistoryBackspace, true)
		let unlistenQuit: (() => void) | null = null
		let gone = false
		void listenQuitRequests((running) => (quitHeld = running))
			.then((stop) => (gone ? stop() : (unlistenQuit = stop)))
			.catch(() => {
				/* not in the desktop shell */
			})

		return () => {
			gone = true
			unlistenQuit?.()
			unwatchScheme()
			unmountCommands()
			window.removeEventListener('keydown', blockHistoryBackspace, true)
		}
	})
</script>

<ToastContext>
	{@render children()}
	<Lazy component={QuickOpen} when={quickOpen.open} fallback="none" />
	<Lazy
		component={ResetConfirmModal}
		when={resetGuard.loss != null}
		fallback="none"
	/>
	<PasswordPromptModal />
	<SshHostKeyModal />
	<ApprovalDock />
	<KeyboardShortcutsModal
		open={appShortcutsUi.open}
		onclose={() => appShortcutsUi.close()}
	/>
	<Modal
		open={quitHeld != null}
		onclose={() => (quitHeld = null)}
		title="Transfers are still running"
		description={quitHeld === 1
			? 'An export or import is still running. Quitting cancels it: a partial export file is removed, and rows an import already wrote stay in the table.'
			: `${quitHeld ?? 0} exports or imports are still running. Quitting cancels them: partial export files are removed, and rows an import already wrote stay in the table.`}
	>
		{#snippet footer()}
			<Button variant="ghost" size="sm" disabled={quitting} onclick={() => (quitHeld = null)}>
				Keep running
			</Button>
			<Button variant="danger" size="sm" disabled={quitting} onclick={() => void quitAnyway()}>
				Cancel and quit
			</Button>
		{/snippet}
	</Modal>
</ToastContext>
