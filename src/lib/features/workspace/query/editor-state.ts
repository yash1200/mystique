/**
 * One CodeMirror state per query tab. The live view calls `setState`.
 * Scroll is not part of EditorState, so it is stored beside it.
 */
import { Compartment, EditorState, type Extension } from '@codemirror/state'
import type { EditorView } from '@codemirror/view'

export const editorCompartments = {
	lang: new Compartment(),
	placeholder: new Compartment(),
	theme: new Compartment(),
	runGutter: new Compartment(),
}

type HeldEditor = {
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

/**
 * Leave `fromId` on the shelf and show `toId`.
 * A tab that has no state yet starts from `seed` and `extensions`.
 * Pass the same extension array the view was created with.
 */
export function switchEditorState(
	view: EditorView,
	fromId: string,
	toId: string,
	seed: string,
	extensions: Extension[],
): EditorState {
	if (fromId && fromId !== toId) {
		holdEditorState(
			fromId,
			view.state,
			view.scrollDOM.scrollTop,
			view.scrollDOM.scrollLeft,
		)
	}
	const existing = held.get(toId)
	if (existing) {
		const top = existing.scrollTop
		const left = existing.scrollLeft
		view.setState(existing.state)
		view.scrollDOM.scrollTop = top
		view.scrollDOM.scrollLeft = left
		return existing.state
	}
	const created = EditorState.create({ doc: seed, extensions })
	view.setState(created)
	view.scrollDOM.scrollTop = 0
	view.scrollDOM.scrollLeft = 0
	return created
}
