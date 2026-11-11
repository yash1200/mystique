<script lang="ts">
	/**
	 * Query editor with CodeMirror 6 syntax highlighting (SQL / MQL).
	 * Highlighting is frontend-only — live as you type, no backend.
	 */
	import { commands, type CommandContext } from '#core/commands'
	import type { CatalogColumnRow } from '#core/contracts'
	import { catalogSignature, formatQuery } from '#engines'
	import { showStatus } from '#features/toast'
	import { appPrefs, cmd } from '#lib/app'
	import { indentWithTab } from '@codemirror/commands'
	import {
		EditorSelection,
		EditorState,
		Prec,
		RangeSetBuilder,
		Transaction,
		type Extension,
	} from '@codemirror/state'
	import {
		placeholder as cmPlaceholder,
		EditorView,
		gutter,
		GutterMarker,
		keymap,
	} from '@codemirror/view'
	import { onMount, untrack } from 'svelte'
	import { mystiqueSearch } from './cm-search-panel'
	import type { CatalogFeed } from './completion-gate'
	import { markDiagnostic, revealDiagnostic } from './diagnostic-view'
	import { loadLanguage, peekLanguage } from './language'
	import {
		coreSetup,
		isLargeDocument,
		richSetup,
	} from './setup'
	import {
		editorCompartments,
		holdEditorState,
		peekEditorState,
		switchEditorState,
	} from './editor-state'
	import {
		createMystiqueEditorTheme,
		identPathHighlight,
		mystiqueHighlight,
	} from './cm-theme'
	import {
		neighbourStatement,
		splitStatementSpans,
		statementAt,
		statementModelFor,
		submissionStart,
		textToRun,
		type StatementSpan,
	} from './statement-span'

	interface Props {
		tabId: string
		connectionId: string
		/** Engine id for dialect-aware formatting (postgres, mysql, sqlite, mongodb, …). */
		engine: string
		placeholder: string
		/** Draft for this tab when it has no EditorState yet. */
		draft?: string
		localText?: string
		language: string
		running: boolean
		/** Large-document mode is on (completion, folding and the run gutter are off). */
		largeDoc?: boolean
		/** One gutter for every engine with `supportsQuery`. */
		showStatementGutter?: boolean
		/** Live catalog columns for schema/table/field completions. */
		catalogColumns?: CatalogColumnRow[]
		/** The tab's catalog right now, so an early completion can wait for it. */
		catalogFeed?: () => CatalogFeed | null
		/** Fired when document text changes. */
		onchange?: (text: string) => void
		/** Caret and document, so a shortcut from outside the editor can run too. */
		onactivity?: (snap: { doc: string; head: number; anchor: number }) => void
		onrun: (request: { text: string; start: number }) => void
		/** Underline in the editor. Null clears it. */
		diagnostic?: { from: number; to: number; message: string } | null
		/** Bumps when Locate error should scroll again. */
		locateNonce?: number
		/** Save query for this connection (name prompt handled by parent). */
		onsave?: () => void
	}

	let {
		tabId,
		connectionId,
		engine,
		placeholder,
		draft = '',
		localText = $bindable(''),
		language,
		running,
		largeDoc = $bindable(false),
		showStatementGutter = true,
		catalogColumns = [],
		catalogFeed,
		onchange,
		onactivity,
		onrun,
		onsave,
		diagnostic = null,
		locateNonce = 0,
	}: Props = $props()

	let hostEl = $state<HTMLDivElement | null>(null)
	let view: EditorView | null = null
	let editorReady = $state(0)
	let scrolledKey = ''
	let revealedNonce = untrack(() => locateNonce)
	let mountedTab = ''
	let appliedLang = ''
	/** Bumps when a language finishes loading, so the editor picks it up. */
	let languagesLoaded = $state(0)
	let extensions: Extension[] = []

	function themeIsDark(): boolean {
		return appPrefs.resolvedTheme === 'dark'
	}
	/** Skip echo when we push parent → editor. */
	let applyingExternal = false

	// Stable bag so CM keymaps/listeners always call latest props.
	const callbacks = {
		onrun: (_request: { text: string; start: number }) => {},
		onformat: () => {},
		onsave: undefined as Props['onsave'],
		onchange: undefined as Props['onchange'],
		onactivity: undefined as Props['onactivity'],
		catalogFeed: undefined as Props['catalogFeed'],
		engine: 'sql',
		language: 'sql',
		running: false,
	}
	$effect(() => {
		callbacks.onrun = onrun
		callbacks.onchange = onchange
		callbacks.onactivity = onactivity
		callbacks.catalogFeed = catalogFeed
		callbacks.engine = engine
		callbacks.language = language
		callbacks.running = running
		callbacks.onformat = formatEditor
		callbacks.onsave = onsave
	})

	class StatementRunMarker extends GutterMarker {
		text: string
		start: number
		constructor(text: string, start: number) {
			super()
			this.text = text
			this.start = start
		}
		toDOM() {
			const btn = document.createElement('button')
			btn.type = 'button'
			btn.className = 'cm-stmt-run'
			btn.title = 'Run statement'
			btn.setAttribute('aria-label', 'Run statement')
			btn.textContent = '▶'
			btn.addEventListener('mousedown', (event) => {
				event.preventDefault()
				event.stopPropagation()
			})
			btn.addEventListener('click', (event) => {
				event.preventDefault()
				event.stopPropagation()
				if (callbacks.running) return
				callbacks.onrun({ text: this.text, start: this.start })
			})
			return btn
		}
	}

	let spanMemo: {
		doc: EditorState['doc']
		model: string
		spans: StatementSpan[]
	} | null = null

	/** Statements of the shown document, split once per document version. */
	function spansOf(state: EditorState): StatementSpan[] {
		const model = statementModelFor(callbacks.engine, callbacks.language)
		if (spanMemo && spanMemo.doc === state.doc && spanMemo.model === model) {
			return spanMemo.spans
		}
		const spans = splitStatementSpans(state.doc.toString(), model)
		spanMemo = { doc: state.doc, model, spans }
		return spans
	}

	function statementGutter(): Extension {
		return gutter({
			class: 'cm-stmt-gutter',
			markers(view) {
				const length = view.state.doc.length
				const spans = spansOf(view.state)
				const builder = new RangeSetBuilder<GutterMarker>()
				let lastLine = -1
				for (const span of spans) {
					const line = view.state.doc.lineAt(Math.min(span.start, length))
					if (line.number === lastLine) continue
					lastLine = line.number
					builder.add(
						line.from,
						line.from,
						new StatementRunMarker(span.text, span.start),
					)
				}
				return builder.finish()
			},
		})
	}

	function emitRun(all: boolean) {
		const doc = view?.state.doc.toString() ?? localText ?? ''
		const head = view?.state.selection.main.head ?? doc.length
		const anchor = view?.state.selection.main.anchor ?? head
		const dialect = statementModelFor(engine, language)
		const text = textToRun(doc, dialect, head, anchor, all)
		if (!text || callbacks.running) return
		callbacks.onrun({
			text,
			start: submissionStart(doc, dialect, head, anchor, all),
		})
	}

	function publishActivity() {
		const doc = view?.state.doc.toString() ?? localText ?? ''
		const head = view?.state.selection.main.head ?? doc.length
		const anchor = view?.state.selection.main.anchor ?? head
		callbacks.onactivity?.({ doc, head, anchor })
	}


	function applyTextToEditor(next: string) {
		const v = view
		if (!v) {
			localText = next
			callbacks.onchange?.(next)
			return
		}
		const cur = v.state.doc.toString()
		if (cur === next) return
		applyingExternal = true
		v.dispatch({
			changes: { from: 0, to: cur.length, insert: next },
			selection: { anchor: Math.min(next.length, v.state.selection.main.head) },
		})
		applyingExternal = false
		localText = next
		callbacks.onchange?.(next)
	}

	/** Run the statement at the caret (or the selection); `all` runs the whole document. */
	export function run(all = false) {
		emitRun(all)
	}

	/** Format the document with the engine's formatter. */
	export function format() {
		void formatEditor()
	}

	async function formatEditor() {
		const src = view?.state.doc.toString() ?? localText ?? ''
		const result = await formatQuery(callbacks.engine, src)
		// The formatter loads on first use; drop the result if the text moved on.
		if ((view?.state.doc.toString() ?? localText ?? '') !== src) return
		if (!result.ok) {
			showStatus(result.error, 'warning')
			return
		}
		if (result.text === src || result.text === src + '\n') return
		applyTextToEditor(result.text)
		view?.focus()
	}

	/**
	 * Language mode, dialect and completions for the engine. Plain text until
	 * the language code arrives, then the reconfigure effect swaps it in.
	 */
	function languageExtension(
		lang: string,
		eng: string,
		cols: CatalogColumnRow[],
	): Extension {
		const make = peekLanguage(lang, eng)
		if (make) return make(cols, () => callbacks.catalogFeed?.() ?? null)
		void loadLanguage(lang, eng).then(
			() => (languagesLoaded += 1),
			() => showStatus('Could not load syntax highlighting.', 'warning'),
		)
		return []
	}

	/** The statement gutter for the current props and document size. */
	function gutterFor(large: boolean): Extension {
		return showStatementGutter && !large ? [statementGutter()] : []
	}

	/**
	 * Put the shown document in or out of large-document mode. `force`
	 * reconfigures even when the mode did not change: a tab's held state
	 * keeps whatever mode it last had.
	 */
	function applyDocMode(v: EditorView, force = false) {
		const doc = v.state.doc
		const was = untrack(() => largeDoc)
		const large = isLargeDocument(doc.length, doc.lines, was)
		if (!force && large === was) return
		largeDoc = large
		v.dispatch({
			effects: [
				editorCompartments.rich.reconfigure(large ? [] : richSetup()),
				editorCompartments.runGutter.reconfigure(gutterFor(large)),
			],
			annotations: Transaction.addToHistory.of(false),
		})
	}

	function lineCountOf(text: string): number {
		let lines = 1
		for (let at = text.indexOf('\n'); at >= 0; at = text.indexOf('\n', at + 1)) {
			lines += 1
		}
		return lines
	}

	let docModeQueued = false
	/** Size checks run after the update; CodeMirror forbids dispatching inside one. */
	function queueDocMode() {
		if (docModeQueued) return
		docModeQueued = true
		queueMicrotask(() => {
			docModeQueued = false
			if (view) applyDocMode(view)
		})
	}

	/** Previous / next statement: move the caret to its start. */
	function gotoStatement(direction: 1 | -1) {
		const v = view
		if (!v) return
		const target = neighbourStatement(
			spansOf(v.state),
			v.state.selection.main.head,
			direction,
		)
		if (!target) return
		v.dispatch({
			selection: EditorSelection.cursor(target.start),
			effects: EditorView.scrollIntoView(target.start, { y: 'nearest' }),
			userEvent: 'select',
		})
		v.focus()
	}

	/** Select the whole statement at the caret. */
	function selectCurrentStatement() {
		const v = view
		if (!v) return
		const span = statementAt(spansOf(v.state), v.state.selection.main.head)
		if (!span) return
		v.dispatch({
			selection: EditorSelection.range(span.start, span.end),
			effects: EditorView.scrollIntoView(span.start, { y: 'nearest' }),
			userEvent: 'select',
		})
		v.focus()
	}

	/** Statement keys act on this editor when focus is in it or in its tab. */
	function inQueryTab({ source, focus }: CommandContext): boolean {
		if (!view) return false
		if (source !== 'key') return true
		const target = focus?.target
		return !!target && (view.dom.contains(target) || !!target.closest('[data-query-editor]'))
	}

	function buildExtensions(lang: string, ph: string): Extension[] {
		return [
			coreSetup(),
			editorCompartments.rich.of(largeDoc ? [] : richSetup()),
			editorCompartments.runGutter.of(gutterFor(largeDoc)),
			mystiqueSearch(),
			editorCompartments.theme.of(createMystiqueEditorTheme(themeIsDark())),
			mystiqueHighlight,
			identPathHighlight,
			editorCompartments.lang.of(
				languageExtension(lang, engine, catalogColumns),
			),
			editorCompartments.placeholder.of(ph ? cmPlaceholder(ph) : []),
			// No linter source. An empty source replaces setDiagnostics after its idle delay.
			// Beat basicSetup's defaultKeymap (Mod-Enter = insertBlankLine).
			Prec.highest(
				keymap.of([
					{
						key: 'Mod-Enter',
						run: () => {
							emitRun(false)
							return true
						},
					},
					{
						key: 'Ctrl-Enter',
						run: () => {
							emitRun(false)
							return true
						},
					},
					{
						key: 'Mod-Shift-Enter',
						run: () => {
							emitRun(true)
							return true
						},
					},
					{
						key: 'Ctrl-Shift-Enter',
						run: () => {
							emitRun(true)
							return true
						},
					},
					{
						key: 'Mod-s',
						run: () => {
							if (callbacks.onsave) {
								callbacks.onsave()
								return true
							}
							return false
						},
					},
					{
						key: 'Mod-Shift-f',
						run: () => {
							callbacks.onformat()
							return true
						},
					},
					{
						// macOS: ⌥⇧F often used by formatters; also bind Alt-Shift-f
						key: 'Alt-Shift-f',
						run: () => {
							callbacks.onformat()
							return true
						},
					},
					indentWithTab,
				]),
			),
			EditorView.updateListener.of((update) => {
				if (update.docChanged && !applyingExternal) {
					const next = update.state.doc.toString()
					localText = next
					callbacks.onchange?.(next)
				}
				if (update.docChanged) queueDocMode()
				if (update.docChanged || update.selectionSet) publishActivity()
			}),
		]
	}

	onMount(() => {
		if (!hostEl) return

		const seed = localText ?? draft ?? ''
		// Decide before the first layout: wrapping a huge line is the slow part.
		largeDoc = isLargeDocument(seed.length, lineCountOf(seed), false)
		extensions = buildExtensions(language, placeholder)
		const existing = peekEditorState(tabId)
		if (existing) {
			view = new EditorView({ state: existing.state, parent: hostEl })
			view.scrollDOM.scrollTop = existing.scrollTop
			view.scrollDOM.scrollLeft = existing.scrollLeft
			applyDocMode(view, true)
		} else {
			view = new EditorView({
				state: EditorState.create({ doc: seed, extensions }),
				parent: hostEl,
			})
		}
		mountedTab = tabId
		editorReady += 1
		publishActivity()

		const unregister = commands.register([
			{ ...cmd.prevStatement, when: inQueryTab, run: () => gotoStatement(-1) },
			{ ...cmd.nextStatement, when: inQueryTab, run: () => gotoStatement(1) },
			{ ...cmd.selectStatement, when: inQueryTab, run: () => selectCurrentStatement() },
		])

		return () => {
			unregister()
			if (view && mountedTab) {
				holdEditorState(
					mountedTab,
					view.state,
					view.scrollDOM.scrollTop,
					view.scrollDOM.scrollLeft,
				)
			}
			view?.destroy()
			view = null
			mountedTab = ''
		}
	})

	$effect(() => {
		void showStatementGutter
		const large = largeDoc
		const v = view
		if (!v) return
		v.dispatch({
			effects: editorCompartments.runGutter.reconfigure(gutterFor(large)),
			annotations: Transaction.addToHistory.of(false),
		})
	})

	$effect(() => {
		const dark = appPrefs.resolvedTheme === 'dark'
		const v = view
		if (!v) return
		v.dispatch({
			effects: editorCompartments.theme.reconfigure(
				createMystiqueEditorTheme(dark),
			),
			annotations: Transaction.addToHistory.of(false),
		})
	})

	// Tab switch restores that tab's EditorState before any document replace.
	// A replace on the previous state would drop its undo stack.
	$effect(() => {
		const id = tabId
		const seed = draft ?? ''
		const next = localText ?? ''
		const v = view
		void editorReady
		if (!v || !id) return
		if (id !== mountedTab) {
			applyingExternal = true
			switchEditorState(v, mountedTab, id, seed, extensions)
			v.dispatch({
				effects: editorCompartments.theme.reconfigure(
					createMystiqueEditorTheme(themeIsDark()),
				),
				annotations: Transaction.addToHistory.of(false),
			})
			applyDocMode(v, true)
			applyingExternal = false
			mountedTab = id
			appliedLang = ''
			const shown = v.state.doc.toString()
			if (shown !== next) {
				localText = shown
				callbacks.onchange?.(shown)
			}
			publishActivity()
			return
		}
		if (v.state.doc.toString() === next) return
		if (v.hasFocus) return
		applyingExternal = true
		v.dispatch({
			changes: { from: 0, to: v.state.doc.length, insert: next },
		})
		applyingExternal = false
	})

	$effect(() => {
		void editorReady
		const range = diagnostic
		const nonce = locateNonce
		const v = view
		if (!v) return
		const placed = markDiagnostic(v, range)
		if (!placed) {
			scrolledKey = ''
			return
		}
		const key = `${nonce}:${placed.from}:${placed.to}`
		if (key === scrolledKey) return
		scrolledKey = key
		// Locate is the only time the editor takes focus.
		const located = nonce !== revealedNonce
		revealedNonce = nonce
		revealDiagnostic(v, placed.from, located)
	})

	// Rebuild the SQL schema only when the catalog contents change.
	$effect(() => {
		const lang = language
		const ph = placeholder
		const eng = engine
		const cols = catalogColumns
		const id = tabId
		void languagesLoaded
		const loaded = peekLanguage(lang, eng) ? 'ready' : 'loading'
		const sig = `${lang}\0${eng}\0${ph}\0${catalogSignature(cols)}\0${loaded}`
		const v = view
		void editorReady
		if (!v || id !== mountedTab) return
		if (sig === appliedLang) return
		appliedLang = sig
		v.dispatch({
			effects: [
				editorCompartments.lang.reconfigure(
					languageExtension(lang, eng, cols),
				),
				editorCompartments.placeholder.reconfigure(
					ph ? cmPlaceholder(ph) : [],
				),
			],
			annotations: Transaction.addToHistory.of(false),
		})
	})
</script>

<div
	class="flex min-h-0 min-w-0 flex-1 flex-col"
	data-query-editor={connectionId}
>
	<label class="sr-only" for="query-cm-{connectionId}">Query editor</label>
	<div
		id="query-cm-{connectionId}"
		bind:this={hostEl}
		class="query-cm-host min-h-0 min-w-0 flex-1 overflow-hidden bg-surface"
		role="textbox"
		aria-multiline="true"
		aria-label="Query editor"
	></div>

</div>

<style>
	/* CodeMirror fills the host; basicSetup adds gutters we style via theme. */
	.query-cm-host :global(.cm-stmt-gutter) {
		width: 1.1rem;
	}
	.query-cm-host :global(.cm-stmt-run) {
		display: grid;
		width: 100%;
		height: 1.1rem;
		place-items: center;
		border: 0;
		padding: 0;
		background: transparent;
		color: var(--text-3);
		font-size: 0.55rem;
		line-height: 1;
		cursor: pointer;
	}
	.query-cm-host :global(.cm-stmt-run:hover) {
		color: var(--accent);
	}
	.query-cm-host :global(.cm-editor) {
		position: relative;
		height: 100%;
	}
	.query-cm-host :global(.cm-scroller) {
		min-height: 100%;
	}
	.query-cm-host :global(.cm-mystique-search) {
		animation: mystique-search-in var(--duration-fast) var(--ease-standard) both;
	}
	@media (prefers-reduced-motion: reduce) {
		.query-cm-host :global(.cm-mystique-search) {
			animation: none;
		}
	}
	@keyframes mystique-search-in {
		from {
			opacity: 0;
			transform: translateY(-4px);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}
</style>
