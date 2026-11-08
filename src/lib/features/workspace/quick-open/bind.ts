import { commands } from '#core/commands'
import { appChrome, cmd } from '#lib/app'
import { cycleConnection, focusExplorerFilter, focusTabIndex } from './actions'
import { quickOpen } from './chrome.svelte'

/** Tab index from a ⌘1…⌘9 keydown. */
function digitIndex(event: KeyboardEvent | null): number | null {
	const m = event?.code.match(/^Digit([1-9])$/)
	return m ? Number(m[1]) - 1 : null
}

/** Quick Open and the navigation commands it owns. Returns the unregister. */
export function registerQuickOpenCommands(): () => void {
	return commands.register([
		{ ...cmd.quickOpen, run: () => quickOpen.toggle() },
		{
			...cmd.toggleSidebar,
			run: () => appChrome.toggle(window.location.pathname),
		},
		{ ...cmd.focusFilter, run: () => focusExplorerFilter() },
		{
			...cmd.focusTab,
			run: ({ args, event }) => {
				const index = args?.index ?? digitIndex(event)
				if (index != null) focusTabIndex(index)
			},
		},
		{ ...cmd.prevConnection, run: () => cycleConnection(-1) },
		{ ...cmd.nextConnection, run: () => cycleConnection(1) },
	])
}
