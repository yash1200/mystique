/**
 * The editor's extension set: `basicSetup` split in two so a very large
 * document can drop the parts that cost time on every keystroke.
 *
 * Large-document mode starts above {@link LARGE_DOC_CHARS} characters or
 * {@link LARGE_DOC_LINES} lines and ends below 90% of both, so typing at the
 * edge does not flip it back and forth. In that mode the editor turns off
 * completion (it reads the whole document), line wrapping (one long line
 * makes every measure slow), bracket and selection-match highlighting, the
 * fold gutter, and the statement gutter.
 */
import {
	autocompletion,
	closeBrackets,
	closeBracketsKeymap,
	completionKeymap,
} from '@codemirror/autocomplete'
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands'
import {
	bracketMatching,
	defaultHighlightStyle,
	foldGutter,
	foldKeymap,
	indentOnInput,
	syntaxHighlighting,
} from '@codemirror/language'
import { lintKeymap } from '@codemirror/lint'
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search'
import { EditorState, type Extension } from '@codemirror/state'
import {
	crosshairCursor,
	drawSelection,
	dropCursor,
	EditorView,
	highlightActiveLine,
	highlightActiveLineGutter,
	highlightSpecialChars,
	keymap,
	lineNumbers,
	rectangularSelection,
} from '@codemirror/view'

export { LARGE_DOC_CHARS, LARGE_DOC_LINES, isLargeDocument } from './large-doc'

/** Always on, whatever the size. */
export function coreSetup(): Extension {
	return [
		lineNumbers(),
		highlightActiveLineGutter(),
		highlightSpecialChars(),
		history(),
		drawSelection(),
		dropCursor(),
		EditorState.allowMultipleSelections.of(true),
		indentOnInput(),
		syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
		closeBrackets(),
		rectangularSelection(),
		crosshairCursor(),
		highlightActiveLine(),
		keymap.of([
			...closeBracketsKeymap,
			...defaultKeymap,
			...searchKeymap,
			...historyKeymap,
			...foldKeymap,
			...completionKeymap,
			...lintKeymap,
		]),
	]
}

/** Off in large-document mode. */
export function richSetup(): Extension {
	return [
		foldGutter(),
		bracketMatching(),
		highlightSelectionMatches(),
		autocompletion({ activateOnTyping: true, icons: true }),
		EditorView.lineWrapping,
	]
}
