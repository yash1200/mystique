/**
 * Open dialogs, newest last. ⌘W closes the top one before it closes a tab.
 * Plain module state: nothing renders from it.
 */

type OpenDialog = {
	close: () => void
	/** False while the dialog must stay open (busy, required choice). */
	dismissible: () => boolean
}

const open: OpenDialog[] = []

/** Register an open dialog. Call the returned function when it closes. */
export function pushDialog(dialog: OpenDialog): () => void {
	open.push(dialog)
	return () => {
		const index = open.indexOf(dialog)
		if (index >= 0) open.splice(index, 1)
	}
}

/**
 * Close the top dialog. True when a dialog was open, even if it refused
 * to close, so the caller does not also close what sits under it.
 */
export function closeTopDialog(): boolean {
	const top = open.at(-1)
	if (!top) return false
	if (top.dismissible()) top.close()
	return true
}

export function openDialogCount(): number {
	return open.length
}
