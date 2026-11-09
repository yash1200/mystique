/**
 * The shelf of per-tab editor states. No CodeMirror runtime here, so the
 * workspace can prune closed tabs before the editor's code has loaded.
 */
import type { EditorState } from '@codemirror/state'

export type HeldEditor = {
	state: EditorState
	scrollTop: number
	scrollLeft: number
}

const held = new Map<string, HeldEditor>()

export function holdEditorState(
	tabId: string,
	state: EditorState,
	scrollTop: number,
	scrollLeft: number,
) {
	if (!tabId) return
	held.set(tabId, { state, scrollTop, scrollLeft })
}

export function peekEditorState(tabId: string): HeldEditor | undefined {
	return held.get(tabId)
}

/** Drop states for tabs that are no longer open. */
export function pruneEditorStates(openIds: readonly string[]) {
	const keep = new Set(openIds)
	for (const id of held.keys()) {
		if (!keep.has(id)) held.delete(id)
	}
}

export function resetEditorStates() {
	held.clear()
}
