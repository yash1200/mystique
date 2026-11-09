/**
 * One CodeMirror state per query tab. The live view calls `setState`.
 * Scroll is not part of EditorState, so it is stored beside it.
 */
import { Compartment, EditorState, type Extension } from '@codemirror/state'
import type { EditorView } from '@codemirror/view'
import { holdEditorState, peekEditorState } from './held'

export {
	holdEditorState,
	peekEditorState,
	pruneEditorStates,
	resetEditorStates,
} from './held'

export const editorCompartments = {
	lang: new Compartment(),
	placeholder: new Compartment(),
	theme: new Compartment(),
	runGutter: new Compartment(),
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
	const existing = peekEditorState(toId)
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
