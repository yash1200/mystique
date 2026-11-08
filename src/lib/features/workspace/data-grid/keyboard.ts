/**
 * Grid keys. Navigation, copy and editing keys a grid handles itself map to
 * one intent here, so both grids read a keydown the same way. Chords that run
 * app commands (⌘N, ⌘D, ⌘R, ⌘Z…) live in the keymap (`#lib/app`) and are
 * registered with `#core/commands`; `gridShortcutBlocked` says when they may run.
 */

export type GridKeyIntent =
	| { kind: 'move'; dRow: number; dCol: number; extend: boolean }
	| { kind: 'select-all' }
	| { kind: 'copy' }
	| { kind: 'escape' }
	/** Shift+Enter: open the cell in the value modal. */
	| { kind: 'expand' }
	/** Enter or F2: edit the focused cell in place. */
	| { kind: 'edit' }
	/** ⌘/Ctrl+Delete or Backspace: mark rows for delete. */
	| { kind: 'delete-rows' }
	/** Delete or Backspace: set cells to NULL. */
	| { kind: 'set-null' }
	/** A printable character: start editing with it. */
	| { kind: 'type'; key: string }

export type GridKeyEvent = {
	key: string
	metaKey: boolean
	ctrlKey: boolean
	altKey: boolean
	shiftKey: boolean
}

const ARROWS: Record<string, [number, number]> = {
	ArrowUp: [-1, 0],
	ArrowDown: [1, 0],
	ArrowLeft: [0, -1],
	ArrowRight: [0, 1],
}

/** What a keydown asks the grid to do, or null for a key the grid ignores. */
export function gridKeyIntent(e: GridKeyEvent): GridKeyIntent | null {
	const mod = e.metaKey || e.ctrlKey
	const { key } = e
	if (mod && !e.altKey && !e.shiftKey) {
		if (key === 'a' || key === 'A') return { kind: 'select-all' }
		if (key === 'c' || key === 'C') return { kind: 'copy' }
		if (key === 'Delete' || key === 'Backspace') return { kind: 'delete-rows' }
	}
	const arrow = ARROWS[key]
	if (arrow) return { kind: 'move', dRow: arrow[0], dCol: arrow[1], extend: e.shiftKey }
	switch (key) {
		case 'Tab':
			return { kind: 'move', dRow: 0, dCol: e.shiftKey ? -1 : 1, extend: false }
		case 'Enter':
			return e.shiftKey ? { kind: 'expand' } : { kind: 'edit' }
		case 'F2':
			return { kind: 'edit' }
		case 'Escape':
			return { kind: 'escape' }
		case 'Delete':
		case 'Backspace':
			return mod || e.altKey ? null : { kind: 'set-null' }
	}
	if (!mod && !e.altKey && key.length === 1) return { kind: 'type', key }
	return null
}

/** Focus is in a text field, where keys belong to the field. */
export function isTypingTarget(target: EventTarget | null | undefined): boolean {
	if (!target || typeof target !== 'object' || !('tagName' in target)) return false
	const el = target as { tagName?: string; isContentEditable?: boolean }
	return isTypingTag(el.tagName, el.isContentEditable)
}

function isTypingTag(tagName: string | undefined, contentEditable: boolean | undefined) {
	const tag = (tagName ?? '').toUpperCase()
	return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || contentEditable === true
}

/** When grid chords (⌘N, ⌘D, ⌘R, ⌘Z…) may run. Ctrl stays with the editor on macOS. */
export function gridShortcutBlocked(input: {
	modalOpen: boolean
	inCellEditor: boolean
	tagName?: string
	contentEditable?: boolean
	inDialog?: boolean
}): boolean {
	if (input.modalOpen || input.inDialog) return true
	if (!isTypingTag(input.tagName, input.contentEditable)) return false
	return !input.inCellEditor
}
