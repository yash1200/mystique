/**
 * Workspace tab shortcuts:
 * - ⌘/Ctrl+W → close active tab
 * - ⌘/Ctrl+T → new query tab
 * - ⌘/Ctrl+S → save active query or table tab
 * Keydown (capture) + optional Tauri menu events (deduped).
 */
export function mountWorkspaceShortcuts(opts: {
	onNewQuery: () => void
	onCloseTab: () => void
	onSaveQuery?: () => void
	onOpenConnection?: () => void
	isTauri: boolean
}): () => void {
	let lastAt = 0
	const runOnce = (fn: () => void) => {
		const now = Date.now()
		if (now - lastAt < 150) return
		lastAt = now
		fn()
	}

	const onKey = (e: KeyboardEvent) => {
		if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey) return
		const code = e.code
		if (code === 'KeyW') {
			e.preventDefault()
			e.stopImmediatePropagation()
			runOnce(opts.onCloseTab)
			return
		}
		if (code === 'KeyT') {
			e.preventDefault()
			e.stopImmediatePropagation()
			runOnce(opts.onNewQuery)
			return
		}
		if (code === 'KeyS' && opts.onSaveQuery) {
			e.preventDefault()
			e.stopImmediatePropagation()
			runOnce(opts.onSaveQuery)
			return
		}
		if (code === 'KeyO' && opts.onOpenConnection) {
			e.preventDefault()
			e.stopImmediatePropagation()
			runOnce(opts.onOpenConnection)
		}
	}

	window.addEventListener('keydown', onKey, true)
	document.addEventListener('keydown', onKey, true)

	const unsubs: Array<() => void> = []
	let cancelled = false
	if (opts.isTauri) {
		void import('@tauri-apps/api/event')
			.then(async ({ listen }) => {
				if (cancelled) return
				const pairs: [string, () => void][] = [
					['workspace-close-tab', opts.onCloseTab],
					['workspace-new-query', opts.onNewQuery],
					['workspace://close-tab', opts.onCloseTab],
					['workspace://new-query', opts.onNewQuery],
				]
				if (opts.onSaveQuery) {
					pairs.push(
						['workspace-save-query', opts.onSaveQuery],
						['workspace://save-query', opts.onSaveQuery],
					)
				}
				if (opts.onOpenConnection) {
					pairs.push(
						['workspace-open-connection', opts.onOpenConnection],
						['workspace://open-connection', opts.onOpenConnection],
					)
				}
				for (const [name, fn] of pairs) {
					if (cancelled) return
					unsubs.push(await listen(name, () => runOnce(fn)))
				}
			})
			.catch(() => {
				/* optional */
			})
	}

	return () => {
		cancelled = true
		window.removeEventListener('keydown', onKey, true)
		document.removeEventListener('keydown', onKey, true)
		for (const u of unsubs) u()
	}
}
