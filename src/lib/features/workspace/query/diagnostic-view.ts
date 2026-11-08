/**
 * Show a query fault in the editor. The token is underlined, never selected:
 * a selection would make the next ⌘Enter run only that token.
 */
import { setDiagnostics } from '@codemirror/lint'
import type { EditorView } from '@codemirror/view'

export type DiagnosticRange = { from: number; to: number; message: string }

/** Underline the fault (or clear it). Returns the clamped range, or null. */
export function markDiagnostic(
	view: EditorView,
	range: DiagnosticRange | null | undefined,
): { from: number; to: number } | null {
	const length = view.state.doc.length
	const from = range ? Math.min(range.from, length) : 0
	const to = range ? Math.min(Math.max(range.to, from), length) : 0
	const marked = !!range && to > from
	view.dispatch(
		setDiagnostics(
			view.state,
			marked ? [{ from, to, severity: 'error', message: range.message }] : [],
		),
	)
	return marked ? { from, to } : null
}

/**
 * Put the cursor at the fault and scroll to it. Focus moves only when the
 * user asked (Locate); otherwise focus stays where it is.
 */
export function revealDiagnostic(view: EditorView, from: number, focus: boolean) {
	view.dispatch({ selection: { anchor: from }, scrollIntoView: true })
	if (focus) view.focus()
}
