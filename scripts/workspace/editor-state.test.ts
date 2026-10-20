import { history, undo } from '@codemirror/commands'
import { EditorState } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import assert from 'node:assert/strict'
import { test } from 'vitest'

import {
	pruneEditorStates,
	resetEditorStates,
	switchEditorState,
} from '../../src/lib/features/workspace/query/editor-state'

test('switching tabs keeps undo, the cursor, and scroll', () => {
	resetEditorStates()
	const parent = document.createElement('div')
	document.body.append(parent)
	const extensions = [history()]
	const view = new EditorView({
		state: EditorState.create({ doc: '', extensions }),
		parent,
	})
	view.dispatch({
		changes: { from: 0, insert: 'hello' },
		selection: { anchor: 5 },
	})
	view.scrollDOM.scrollTop = 18
	view.scrollDOM.scrollLeft = 6

	switchEditorState(view, 'tab-a', 'tab-b', 'other', extensions)
	assert.equal(view.state.doc.toString(), 'other')

	switchEditorState(view, 'tab-b', 'tab-a', '', extensions)
	assert.equal(view.state.doc.toString(), 'hello')
	assert.equal(view.state.selection.main.head, 5)
	assert.equal(view.scrollDOM.scrollTop, 18)
	assert.equal(view.scrollDOM.scrollLeft, 6)
	undo(view)
	assert.equal(view.state.doc.toString(), '')

	view.destroy()
	parent.remove()
	pruneEditorStates(['tab-b'])
	resetEditorStates()
})
