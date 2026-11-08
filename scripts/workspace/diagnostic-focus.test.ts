import { forEachDiagnostic } from '@codemirror/lint'
import { EditorView } from '@codemirror/view'
import { afterEach, describe, expect, it } from 'vitest'

import {
	markDiagnostic,
	revealDiagnostic,
} from '../../src/lib/features/workspace/query/editor/diagnostic-view'

const cleanup: Array<() => void> = []
afterEach(() => {
	for (const done of cleanup.splice(0)) done()
})

function editor(doc: string): EditorView {
	const host = document.createElement('div')
	document.body.append(host)
	const view = new EditorView({ doc, parent: host })
	cleanup.push(() => {
		view.destroy()
		host.remove()
	})
	return view
}

describe('R-28 error diagnostics', () => {
	it('underlines the token without selecting it', () => {
		const doc = 'SELECT 1;\nSELECT * FORM t'
		const view = editor(doc)
		const from = doc.indexOf('FORM')
		const placed = markDiagnostic(view, { from, to: from + 4, message: 'syntax' })
		expect(placed).toEqual({ from, to: from + 4 })
		revealDiagnostic(view, from, false)
		const sel = view.state.selection.main
		expect(sel.empty).toBe(true)
		expect(sel.head).toBe(from)
		const marked: string[] = []
		forEachDiagnostic(view.state, (_d, start, end) => {
			marked.push(view.state.doc.sliceString(start, end))
		})
		expect(marked).toEqual(['FORM'])
	})

	it('does not take focus from another control after a run', () => {
		const view = editor('SELECT * FORM t')
		const other = document.createElement('input')
		document.body.append(other)
		cleanup.push(() => other.remove())
		other.focus()
		revealDiagnostic(view, 9, false)
		expect(document.activeElement).toBe(other)
		expect(view.hasFocus).toBe(false)
	})

	it('clears the underline when there is no fault', () => {
		const view = editor('SELECT 1')
		markDiagnostic(view, { from: 0, to: 6, message: 'x' })
		expect(markDiagnostic(view, null)).toBeNull()
		let count = 0
		forEachDiagnostic(view.state, () => count++)
		expect(count).toBe(0)
	})
})
