<script lang="ts">
	import { isTauri } from '$core/ipc'
	import {
		applyPreferredUiScale,
		applyUiScale,
		setAppTitle,
		writeScalePreference,
	} from '$core/util'
	import { ToastContext } from '$features/toast'
	import { resetAppToHome } from '$features/workspace/shell'
	import { appPrefs } from '$lib/app'
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

		const unsubs: Array<() => void> = []
		let cancelled = false
		if (isTauri()) {
			void import('@tauri-apps/api/event')
				.then(async ({ listen }) => {
					if (cancelled) return
					unsubs.push(await listen('ui-zoom-in', () => zoomBy(0.1)))
					unsubs.push(await listen('ui-zoom-out', () => zoomBy(-0.1)))
					unsubs.push(await listen('ui-zoom-reset', () => zoomReset()))
				})
				.catch(() => {
					/* optional */
				})
		}

		return () => {
			cancelled = true
			unwatchScheme()
			window.removeEventListener('keydown', onKey, true)
			for (const u of unsubs) u()
		}
	})
</script>

<ToastContext>
	{@render children()}
</ToastContext>
