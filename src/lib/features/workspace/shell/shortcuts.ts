/**
 * Workspace tab commands: ⌘T new query, ⌘S save, ⌘W close, ⌘⇧T reopen
 * closed tab, ⌘O open connection.
 *
 * In the desktop app the native menu accelerator is the only source, so each
 * chord runs once. The browser preview has no menu and uses keydown instead.
 */
import { isMacPlatform, isModChord } from '#core/util'

export type WorkspaceCommand =
	| 'new-query'
	| 'save'
	| 'close-tab'
	| 'reopen-tab'
	| 'open-connection'

/** Event the native menu emits for each command (menu.rs). */
export const WORKSPACE_MENU_EVENTS: Record<WorkspaceCommand, string> = {
	'new-query': 'workspace-new-query',
	save: 'workspace-save-query',
	'close-tab': 'workspace-close-tab',
	'reopen-tab': 'workspace-reopen-tab',
	'open-connection': 'workspace-open-connection',
}

type ChordKey = {
	metaKey: boolean
	ctrlKey: boolean
	altKey: boolean
	shiftKey: boolean
	code: string
	repeat?: boolean
}

/** Keydown → command. Used only where there is no native menu. */
export function workspaceChord(event: ChordKey, mac: boolean): WorkspaceCommand | null {
	if (event.altKey || event.repeat) return null
	if (!isModChord(event, mac)) return null
	if (event.shiftKey) return event.code === 'KeyT' ? 'reopen-tab' : null
	if (event.code === 'KeyT') return 'new-query'
	if (event.code === 'KeyS') return 'save'
	if (event.code === 'KeyW') return 'close-tab'
	if (event.code === 'KeyO') return 'open-connection'
	return null
}

export function mountWorkspaceShortcuts(opts: {
	commands: Partial<Record<WorkspaceCommand, () => void>>
	isTauri: boolean
	mac?: boolean
}): () => void {
	const { commands } = opts
	if (!opts.isTauri) {
		const mac = opts.mac ?? isMacPlatform()
		const onKey = (event: KeyboardEvent) => {
			const command = workspaceChord(event, mac)
			const run = command ? commands[command] : undefined
			if (!run) return
			event.preventDefault()
			event.stopImmediatePropagation()
			run()
		}
		window.addEventListener('keydown', onKey, true)
		return () => window.removeEventListener('keydown', onKey, true)
	}

	const unsubs: Array<() => void> = []
	let cancelled = false
	void import('@tauri-apps/api/event')
		.then(async ({ listen }) => {
			for (const [command, event] of Object.entries(WORKSPACE_MENU_EVENTS)) {
				const run = commands[command as WorkspaceCommand]
				if (!run || cancelled) continue
				const unlisten = await listen(event, () => run())
				if (cancelled) unlisten()
				else unsubs.push(unlisten)
			}
		})
		.catch(() => {
			/* optional */
		})
	return () => {
		cancelled = true
		for (const u of unsubs) u()
	}
}
