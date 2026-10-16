<script lang="ts">
	import { isTauri, listenNativeMenu } from '#core/ipc'
	import {
		applyPreferredUiScale,
		applyUiScale,
		blockHistoryBackspace,
		setAppTitle,
		writeScalePreference,
	} from '#core/util'
	import { AboutModal, appAboutUi, appSettingsUi } from '#features/settings'
	import { appShortcutsUi, KeyboardShortcutsModal } from '#features/shortcuts'
	import { ToastContext } from '#features/toast'
	import { mountAppChords, QuickOpen } from '#features/workspace/quick-open'
	import { resetAppToHome } from '#features/workspace/shell'
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

		const onKey = (e: KeyboardEvent) => {
			if (!(e.ctrlKey || e.metaKey) || e.altKey) return
			if (e.shiftKey && e.code === 'KeyR') {
				e.preventDefault()
				e.stopImmediatePropagation()
				void resetAppToHome()
				return
			}
			if (!e.shiftKey && (e.key === ',' || e.code === 'Comma')) {
				e.preventDefault()
				e.stopImmediatePropagation()
				appSettingsUi.show()
				return
			}
			if (e.shiftKey) return
			if (e.key === '=' || e.key === '+') {
				e.preventDefault()
				zoomBy(0.1)
			} else if (e.key === '-' || e.key === '_') {
				e.preventDefault()
				zoomBy(-0.1)
			} else if (e.key === '0') {
				e.preventDefault()
				zoomReset()
			}
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
				'app-open-about': () => appAboutUi.show(),
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
	<AboutModal />
	<QuickOpen />
	<KeyboardShortcutsModal
		open={appShortcutsUi.open}
		onclose={() => appShortcutsUi.close()}
	/>
</ToastContext>
