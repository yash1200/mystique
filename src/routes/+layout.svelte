<script lang="ts">
	import { commands, mountCommands } from '#core/commands'
	import {
		applyPreferredUiScale,
		applyUiScale,
		blockHistoryBackspace,
		setAppTitle,
		writeScalePreference,
	} from '#core/util'
	import { PasswordPromptModal } from '#features/connections'
	import { appSettingsUi, type SettingsSection } from '#features/settings'
	import { appShortcutsUi, KeyboardShortcutsModal } from '#features/shortcuts'
	import { ToastContext } from '#features/toast'
	import {
		QuickOpen,
		registerQuickOpenCommands,
	} from '#features/workspace/quick-open'
	import { resetGuard } from '#features/workspace/session'
	import { ResetConfirmModal } from '#features/workspace/shell'
	import { appPrefs, cmd, KEYMAP } from '#lib/app'
	import { onMount } from 'svelte'
	import '../app.css'

	let { children } = $props()

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

	onMount(() => {
		appPrefs.apply()
		const unwatchScheme = appPrefs.watchSystemTheme()
		applyPreferredUiScale()
		void setAppTitle()

		const unmountCommands = mountCommands()
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
		window.addEventListener('keydown', blockHistoryBackspace, true)

		return () => {
			unwatchScheme()
			unmountCommands()
			unregister()
			unregisterQuickOpen()
			window.removeEventListener('keydown', blockHistoryBackspace, true)
		}
	})
</script>

<ToastContext>
	{@render children()}
	<QuickOpen />
	<ResetConfirmModal />
	<PasswordPromptModal />
	<KeyboardShortcutsModal
		open={appShortcutsUi.open}
		onclose={() => appShortcutsUi.close()}
	/>
</ToastContext>
