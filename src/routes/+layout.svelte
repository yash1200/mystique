<script lang="ts">
	import { isTauri, listenNativeMenu } from '#core/ipc'
	import {
		applyPreferredUiScale,
		applyUiScale,
		blockHistoryBackspace,
		isMacPlatform,
		setAppTitle,
		writeScalePreference,
	} from '#core/util'
	import { PasswordPromptModal } from '#features/connections'
	import { appSettingsUi } from '#features/settings'
	import {
		appKeyCommand,
		appShortcutsUi,
		KeyboardShortcutsModal,
	} from '#features/shortcuts'
	import { ToastContext } from '#features/toast'
	import { mountAppChords, QuickOpen } from '#features/workspace/quick-open'
	import { resetGuard } from '#features/workspace/session'
	import { ResetConfirmModal } from '#features/workspace/shell'
	import { appPrefs } from '#lib/app'
	import { onMount } from 'svelte'
	import '../app.css'

	let { children } = $props()

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

		const keyContext = { mac: isMacPlatform(), nativeMenu: isTauri() }
		const onKey = (e: KeyboardEvent) => {
			const command = appKeyCommand(e, keyContext)
			if (!command) return
			e.preventDefault()
			if (command === 'reset') {
				e.stopImmediatePropagation()
				resetGuard.request()
			} else if (command === 'settings') {
				e.stopImmediatePropagation()
				appSettingsUi.show()
			} else if (command === 'zoom-in') zoomBy(0.1)
			else if (command === 'zoom-out') zoomBy(-0.1)
			else zoomReset()
		}
		window.addEventListener('keydown', onKey, true)
		window.addEventListener('keydown', blockHistoryBackspace, true)
		const unsubChords = mountAppChords()

		const unsubs: Array<() => void> = []
		let cancelled = false
		if (isTauri()) {
			void listenNativeMenu({
				'ui-zoom-in': () => zoomBy(0.1),
				'ui-zoom-out': () => zoomBy(-0.1),
				'ui-zoom-reset': zoomReset,
				'app-open-settings': () => appSettingsUi.show(),
				'app-open-shortcuts': () => appShortcutsUi.show(),
			})
				.then((u) => {
					if (cancelled) u()
					else unsubs.push(u)
				})
				.catch(() => {
					/* optional */
				})
		}

		return () => {
			cancelled = true
			unwatchScheme()
			window.removeEventListener('keydown', onKey, true)
			window.removeEventListener('keydown', blockHistoryBackspace, true)
			unsubChords()
			for (const u of unsubs) u()
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
