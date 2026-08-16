/**
 * CodeMirror theme + highlight styles bound to Mystique design tokens
 * so light/dark themes stay consistent without a separate CM theme package.
 */
import {
	HighlightStyle,
	syntaxHighlighting,
	syntaxTree,
} from '@codemirror/language'
import { Prec, RangeSetBuilder } from '@codemirror/state'
import {
	Decoration,
	EditorView,
	ViewPlugin,
	type DecorationSet,
	type ViewUpdate,
} from '@codemirror/view'
import { tags as t } from '@lezer/highlight'

/** First matching spec wins — more specific tags before generic ones. */
export const mystiqueHighlight = Prec.high(
	syntaxHighlighting(
		HighlightStyle.define([
			{
				tag: t.function(t.variableName),
				color: 'var(--syntax-func)',
				fontWeight: '600',
			},
			{
				tag: t.function(t.propertyName),
				color: 'var(--syntax-func)',
				fontWeight: '600',
			},
			{
				tag: t.function(t.definition(t.variableName)),
				color: 'var(--syntax-func)',
				fontWeight: '600',
			},
			{
				tag: t.standard(t.name),
				color: 'var(--syntax-func)',
				fontWeight: '600',
			},
			{ tag: t.special(t.name), color: 'var(--syntax-property)' },
			{ tag: t.special(t.propertyName), color: 'var(--syntax-property)' },
			/* SQL "quoted ids"; JS template strings are rare in MQL. */
			{ tag: t.special(t.string), color: 'var(--syntax-name)' },
			{
				tag: t.keyword,
				color: 'var(--syntax-keyword)',
				fontWeight: '700',
			},
			{
				tag: t.moduleKeyword,
				color: 'var(--syntax-keyword)',
				fontWeight: '700',
			},
			{
				tag: t.controlKeyword,
				color: 'var(--syntax-keyword)',
				fontWeight: '700',
			},
			{
				tag: t.definitionKeyword,
				color: 'var(--syntax-keyword)',
				fontWeight: '700',
			},
			{
				tag: t.modifier,
				color: 'var(--syntax-keyword)',
				fontWeight: '600',
			},
			{
				tag: t.operatorKeyword,
				color: 'var(--syntax-keyword)',
				fontWeight: '700',
			},
			{ tag: t.bool, color: 'var(--syntax-atom)', fontWeight: '600' },
			{ tag: t.null, color: 'var(--syntax-atom)', fontWeight: '600' },
			{ tag: t.atom, color: 'var(--syntax-atom)', fontWeight: '600' },
			{ tag: t.self, color: 'var(--syntax-atom)', fontWeight: '600' },
			{ tag: t.string, color: 'var(--syntax-string)' },
			{ tag: t.regexp, color: 'var(--syntax-string)' },
			{ tag: t.escape, color: 'var(--syntax-number)' },
			{ tag: t.number, color: 'var(--syntax-number)' },
			{ tag: t.integer, color: 'var(--syntax-number)' },
			{ tag: t.float, color: 'var(--syntax-number)' },
			{
				tag: t.comment,
				color: 'var(--syntax-comment)',
				fontStyle: 'italic',
			},
			{
				tag: t.lineComment,
				color: 'var(--syntax-comment)',
				fontStyle: 'italic',
			},
			{
				tag: t.blockComment,
				color: 'var(--syntax-comment)',
				fontStyle: 'italic',
			},
			{
				tag: t.docComment,
				color: 'var(--syntax-comment)',
				fontStyle: 'italic',
			},
			{ tag: t.operator, color: 'var(--syntax-operator)' },
			{ tag: t.compareOperator, color: 'var(--syntax-operator)' },
			{ tag: t.logicOperator, color: 'var(--syntax-operator)' },
			{ tag: t.arithmeticOperator, color: 'var(--syntax-operator)' },
			{ tag: t.bitwiseOperator, color: 'var(--syntax-operator)' },
			{ tag: t.updateOperator, color: 'var(--syntax-operator)' },
			{ tag: t.definitionOperator, color: 'var(--syntax-operator)' },
			{ tag: t.derefOperator, color: 'var(--syntax-operator)' },
			{ tag: t.punctuation, color: 'var(--syntax-punct)' },
			{ tag: t.separator, color: 'var(--syntax-punct)' },
			{ tag: t.bracket, color: 'var(--syntax-punct)' },
			{ tag: t.paren, color: 'var(--syntax-punct)' },
			{ tag: t.squareBracket, color: 'var(--syntax-punct)' },
			{ tag: t.brace, color: 'var(--syntax-punct)' },
			{ tag: t.angleBracket, color: 'var(--syntax-punct)' },
			{ tag: t.special(t.brace), color: 'var(--syntax-punct)' },
			{ tag: t.name, color: 'var(--syntax-name)' },
			{ tag: t.variableName, color: 'var(--syntax-name)' },
			{ tag: t.definition(t.variableName), color: 'var(--syntax-name)' },
			{ tag: t.labelName, color: 'var(--syntax-property)' },
			{ tag: t.propertyName, color: 'var(--syntax-property)' },
			{ tag: t.definition(t.propertyName), color: 'var(--syntax-property)' },
			{ tag: t.attributeName, color: 'var(--syntax-property)' },
			{ tag: t.typeName, color: 'var(--syntax-type)' },
			{ tag: t.definition(t.typeName), color: 'var(--syntax-type)' },
			{ tag: t.className, color: 'var(--syntax-type)' },
			{ tag: t.namespace, color: 'var(--syntax-type)' },
			{ tag: t.macroName, color: 'var(--syntax-func)' },
			{ tag: t.meta, color: 'var(--syntax-comment)' },
			{ tag: t.invalid, color: 'var(--syntax-invalid)' },
		]),
	),
)

function isSqlIdent(name: string) {
	return name === 'Identifier' || name === 'QuotedIdentifier'
}

const identHeadMark = Decoration.mark({ class: 'cm-ident-head' })
const identLeafMark = Decoration.mark({ class: 'cm-ident-leaf' })

/** Color `schema.table.col` paths: qualifier vs field. Bare aliases stay `--syntax-name`. */
function identPathDecos(view: EditorView): DecorationSet {
	const builder = new RangeSetBuilder<Decoration>()
	const tree = syntaxTree(view.state)
	for (const { from, to } of view.visibleRanges) {
		tree.iterate({
			from,
			to,
			enter(node) {
				if (!isSqlIdent(node.name)) return
				if (node.node.prevSibling?.name === '.') {
					builder.add(node.from, node.to, identLeafMark)
				} else if (node.node.nextSibling?.name === '.') {
					builder.add(node.from, node.to, identHeadMark)
				}
			},
		})
	}
	return builder.finish()
}

export const identPathHighlight = ViewPlugin.fromClass(
	class {
		decorations: DecorationSet
		constructor(view: EditorView) {
			this.decorations = identPathDecos(view)
		}
		update(update: ViewUpdate) {
			if (update.docChanged || update.viewportChanged) {
				this.decorations = identPathDecos(update.view)
			}
		}
	},
	{ decorations: (v) => v.decorations },
)

const editorThemeSpec = {
	'&': {
		height: '100%',
		fontSize: '0.8125rem' /* text-body */,
		backgroundColor: 'var(--bg-input)',
		color: 'var(--text-primary)',
	},
	'&.cm-focused': {
		outline: 'none',
	},
	'.cm-scroller': {
		fontFamily: 'var(--mds-font-mono)',
		lineHeight: '1.625',
		overflow: 'auto',
	},
	'.cm-content': {
		caretColor: 'var(--accent)',
		/* Tight left padding so text sits close to line numbers */
		padding: '0.5rem 0.75rem 0.5rem 0.35rem',
		minHeight: '100%',
	},
	'.cm-line': {
		padding: '0 1px',
	},
	'.cm-cursor, .cm-dropCursor': {
		borderLeftColor: 'var(--accent)',
	},
	'&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection':
		{
			backgroundColor: 'var(--bg-selected) !important',
		},
	'.cm-activeLine': {
		backgroundColor: 'var(--bg-hover)',
	},
	'.cm-gutters': {
		backgroundColor: 'var(--bg-elevated)',
		color: 'var(--text-muted)',
		border: 'none',
		borderRight: '1px solid var(--border-subtle)',
		minWidth: '1.75rem',
	},
	'.cm-gutterElement': {
		/* Less space between the number and the text */
		padding: '0 0.25rem 0 0.2rem',
		minWidth: '1.5rem',
	},
	'.cm-lineNumbers .cm-gutterElement': {
		textAlign: 'right',
		paddingRight: '0.25rem',
	},
	'.cm-activeLineGutter': {
		backgroundColor: 'var(--bg-hover)',
		color: 'var(--text-secondary)',
	},
	'.cm-foldPlaceholder': {
		backgroundColor: 'var(--bg-input)',
		border: 'none',
		color: 'var(--text-muted)',
	},
	'.cm-tooltip': {
		backgroundColor: 'var(--bg-elevated)',
		border: '1px solid var(--border-subtle)',
		color: 'var(--text-primary)',
		borderRadius: '5px',
		zIndex: '40',
	},
	'.cm-tooltip.cm-tooltip-autocomplete': {
		backgroundColor: 'var(--bg-elevated)',
	},
	'.cm-tooltip.cm-tooltip-autocomplete > ul': {
		fontFamily: 'var(--mds-font-mono)',
		fontSize: '0.8125rem',
		maxHeight: '16rem',
	},
	'.cm-tooltip.cm-tooltip-autocomplete > ul > li': {
		padding: '0.2rem 0.5rem',
	},
	'.cm-tooltip-autocomplete ul li[aria-selected]': {
		backgroundColor: 'var(--bg-selected)',
		color: 'var(--text-primary)',
	},
	'.cm-completionLabel': {
		color: 'var(--text-primary)',
	},
	'.cm-completionDetail': {
		color: 'var(--text-muted)',
		fontStyle: 'normal',
		marginLeft: '0.5rem',
	},
	'.cm-completionIcon': {
		opacity: '0.7',
	},
	'.cm-panels': {
		backgroundColor: 'var(--bg-elevated)',
		color: 'var(--text-primary)',
	},
	'.cm-placeholder': {
		color: 'var(--text-muted)',
		fontStyle: 'normal',
	},
	/* Beat highlight token color on the inner span. */
	'.cm-ident-head, .cm-ident-head span': {
		color: 'var(--syntax-name) !important',
	},
	'.cm-ident-leaf, .cm-ident-leaf span': {
		color: 'var(--syntax-property) !important',
	},
}

/** Editor chrome bound to CSS tokens. Pass `dark` from `data-theme`. */
export function createMystiqueEditorTheme(dark: boolean) {
	return EditorView.theme(editorThemeSpec, { dark })
}
