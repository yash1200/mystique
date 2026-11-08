/**
 * Focus scopes. A keydown belongs to where focus is: a code editor
 * (CodeMirror), a text field, a grid, or anywhere while a dialog is open.
 * A command says which of these it may fire in.
 */
import { isWritableField } from '../util/keyboard'

export type Focus = {
	target: Element | null
	/** Focus is inside a CodeMirror editor. */
	editor: boolean
	/** Focus is in a text field that is not a code editor. */
	input: boolean
	/** Focus is inside a grid (`role="grid"`). */
	grid: boolean
	/** A dialog is open (dialog stack), or focus is inside one. */
	dialog: boolean
}

/**
 * Where a command may fire from the keyboard. Defaults: editor, input and grid
 * allowed; blocked while a dialog is open.
 */
export type Scope = {
	editor?: boolean
	input?: boolean
	grid?: boolean
	dialog?: boolean
}

export function focusOf(target: EventTarget | null, dialogOpen: boolean): Focus {
	const el = target instanceof Element ? target : null
	const editor = !!el?.closest('.cm-editor')
	return {
		target: el,
		editor,
		input: !editor && isWritableField(target),
		grid: !!el?.closest('[role="grid"]'),
		dialog: dialogOpen || !!el?.closest('[role="dialog"]'),
	}
}

export function allowedIn(scope: Scope | undefined, focus: Focus): boolean {
	if (focus.dialog && !(scope?.dialog ?? false)) return false
	if (focus.editor && !(scope?.editor ?? true)) return false
	if (focus.input && !(scope?.input ?? true)) return false
	if (focus.grid && !(scope?.grid ?? true)) return false
	return true
}
