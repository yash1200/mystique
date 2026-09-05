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
		backgroundColor: 'transparent',
		color: 'var(--text-primary)',
		fontFamily: 'var(--mds-font-ui)',
	},
	'.cm-mystique-search-slot': {
		display: 'none',
	},
	'.cm-panels.cm-panels-top': {
		height: '0',
		minHeight: '0',
		overflow: 'hidden',
		border: 'none',
		padding: '0',
		backgroundColor: 'transparent',
	},
	'.cm-searchMatch': {
		backgroundColor: 'var(--accent-muted)',
	},
	'.cm-searchMatch-selected': {
		backgroundColor: 'var(--bg-selected)',
		outline: '1px solid color-mix(in srgb, var(--accent) 55%, transparent)',
	},
	'.cm-selectionMatch': {
		backgroundColor: 'var(--bg-hover)',
	},
	'.cm-mystique-search': {
		position: 'absolute',
		top: '0.5rem',
		right: '0.5rem',
		zIndex: '20',
		display: 'grid',
		width: 'min(28rem, calc(100% - 1rem))',
		gridTemplateColumns: '1.5rem minmax(0, 1fr) auto',
		gridTemplateRows: 'auto auto',
		alignItems: 'center',
		columnGap: '0.25rem',
		rowGap: '0.25rem',
		border: '1px solid var(--border-subtle)',
		borderRadius: 'var(--mds-radius-lg)',
		backgroundColor: 'var(--bg-elevated)',
		padding: '0.375rem 0.375rem 0.375rem 0.25rem',
		boxShadow: 'var(--mds-shadow-md)',
		color: 'var(--text-primary)',
		fontFamily: 'var(--mds-font-ui)',
	},
	'.cm-mystique-search-collapsed': {
		gridTemplateRows: 'auto',
	},
	'.cm-mystique-search-twist': {
		gridColumn: '1',
		gridRow: '1 / span 2',
		alignSelf: 'start',
		marginTop: '0.125rem',
	},
	'.cm-mystique-search-collapsed .cm-mystique-search-twist': {
		gridRow: '1',
	},
	'.cm-mystique-search-fieldwrap': {
		position: 'relative',
		display: 'flex',
		minWidth: '8rem',
		gridColumn: '2',
		alignItems: 'center',
	},
	'.cm-mystique-search-meta, .cm-mystique-search-actions': {
		display: 'flex',
		gridColumn: '3',
		alignItems: 'center',
		gap: '0.0625rem',
		paddingLeft: '0.25rem',
	},
	'.cm-mystique-search-field': {
		width: '100%',
		height: '1.75rem',
		margin: '0',
		border: '1px solid transparent',
		borderRadius: 'var(--mds-radius-md)',
		backgroundColor: 'var(--bg-input)',
		padding: '0 5.25rem 0 0.55rem',
		color: 'var(--text-primary)',
		fontFamily: 'var(--mds-font-ui)',
		fontSize: 'var(--text-label)',
		lineHeight: 'var(--text-label--line-height)',
		outline: 'none',
		appearance: 'none',
	},
	'.cm-mystique-search-fieldwrap:has(.cm-mystique-search-infield :only-child) .cm-mystique-search-field':
		{
			paddingRight: '2rem',
		},
	'.cm-mystique-search-field::placeholder': {
		color: 'var(--text-muted)',
	},
	'.cm-mystique-search-field:focus-visible': {
		borderColor: 'var(--border-focus)',
		boxShadow: '0 0 0 2px var(--accent-muted)',
	},
	'.cm-mystique-search-field.cm-mystique-search-bad': {
		borderColor: 'var(--status-danger)',
	},
	'.cm-mystique-search-field::-webkit-search-cancel-button, .cm-mystique-search-field::-webkit-search-decoration':
		{
			appearance: 'none',
		},
	'.cm-mystique-search-infield': {
		position: 'absolute',
		right: '0.2rem',
		display: 'flex',
		alignItems: 'center',
		gap: '0.0625rem',
	},
	'.cm-mystique-search-count': {
		minWidth: '4.25rem',
		padding: '0 0.4rem',
		color: 'var(--text-muted)',
		fontFamily: 'var(--mds-font-ui)',
		fontSize: 'var(--text-caption)',
		fontVariantNumeric: 'tabular-nums',
		textAlign: 'right',
		whiteSpace: 'nowrap',
	},
	'.cm-mystique-search-count-bad': {
		color: 'var(--status-danger)',
	},
	'.cm-mystique-search-toggle, .cm-mystique-search-icon': {
		display: 'inline-grid',
		flex: '0 0 auto',
		placeItems: 'center',
		height: '1.5rem',
		margin: '0',
		border: '1px solid transparent',
		borderRadius: 'var(--mds-radius-sm)',
		backgroundColor: 'transparent',
		padding: '0 0.28rem',
		color: 'var(--text-muted)',
		fontFamily: 'var(--mds-font-ui)',
		fontSize: 'var(--text-micro)',
		fontWeight: '600',
		letterSpacing: '0.01em',
		lineHeight: '1',
		cursor: 'pointer',
	},
	'.cm-mystique-search-icon': {
		width: '1.5rem',
		padding: '0',
	},
	'.cm-mystique-search-icon svg': {
		width: '0.875rem',
		height: '0.875rem',
	},
	'.cm-mystique-search-word': {
		textDecoration: 'underline',
		textUnderlineOffset: '0.12em',
	},
	'.cm-mystique-search-toggle:hover, .cm-mystique-search-icon:hover': {
		backgroundColor: 'var(--bg-hover)',
		color: 'var(--text-primary)',
	},
	'.cm-mystique-search-toggle:focus-visible, .cm-mystique-search-icon:focus-visible':
		{
			borderColor: 'var(--border-focus)',
			outline: 'none',
			boxShadow: '0 0 0 2px var(--accent-muted)',
		},
	'.cm-mystique-search-on': {
		backgroundColor: 'var(--bg-hover)',
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
