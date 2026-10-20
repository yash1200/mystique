<script lang="ts">
	/**
	 * Query editor with CodeMirror 6 syntax highlighting (SQL / MQL).
	 * Highlighting is frontend-only — live as you type, no backend.
	 */
	import type { CatalogColumnRow } from '#core/contracts'
	import { Button, Icon, Input, Pill } from '#core/ui'
	import { adapterFor, catalogSignature, formatQuery } from '#engines'
	import { showToast } from '#features/toast'
	import { appPrefs } from '#lib/app'
	import { autocompletion } from '@codemirror/autocomplete'
	import { indentWithTab } from '@codemirror/commands'
	import { javascript, javascriptLanguage } from '@codemirror/lang-javascript'
	import { sql, StandardSQL, type SQLNamespace } from '@codemirror/lang-sql'
	import { setDiagnostics } from '@codemirror/lint'
	import {
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
	import { basicSetup } from 'codemirror'
	import { onMount } from 'svelte'
	import { mystiqueSearch } from '../cm-search-panel'
	import {
		editorCompartments,
		holdEditorState,
		peekEditorState,
		switchEditorState,
	} from '../editor-state'
	import {
		createMystiqueEditorTheme,
		identPathHighlight,
		mystiqueHighlight,
	} from '../cm-theme'
	import {
		splitDialectFor,
		splitStatementSpans,
		submissionStart,
		textToRun,
	} from '../statement-span'

	interface Props {
		tabId: string
		connectionId: string
		/** Engine id for dialect-aware formatting (postgres, mysql, sqlite, mongodb, …). */
		engine: string
		placeholder: string
		/** Draft for this tab when it has no EditorState yet. */
		draft?: string
		localText?: string
		lineCount: number
		language: string
		collection?: string
		canRun: boolean
		running: boolean
		/** Engine advertises cancel. The button stays disabled while running otherwise. */
		canCancel?: boolean
		oncancel?: () => void
		/** One gutter for every engine with `supportsQuery`. */
		showStatementGutter?: boolean
		/** True when draft differs from last saved baseline. */
		dirty?: boolean
		/** True while a save is in flight. */
		saving?: boolean
		/** Live catalog columns for schema/table/field completions. */
		catalogColumns?: CatalogColumnRow[]
		/** Fired when document text changes. */
		onchange?: (text: string) => void
		/** Caret and document, so a shortcut from outside the editor can run too. */
		onactivity?: (snap: { doc: string; head: number; anchor: number }) => void
		onrun: (request: { text: string; start: number }) => void
		/** Explain button. Comes from `supportsExplainPlan`. */
		showExplain?: boolean
		/** Analyze collects actual rows. Off until the user turns it on. */
		analyze?: boolean
		onanalyze?: (on: boolean) => void
		onexplain?: () => void
		/** Engine can hold a transaction on one connection. */
		showTransactions?: boolean
		/** Off means the tab is pinned and auto-commit is off. */
		manualCommit?: boolean
		transactionOpen?: boolean
		onmanualcommit?: (manual: boolean) => void
		oncommit?: () => void
		onrollback?: () => void
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
		lineCount,
		language,
		collection = $bindable(''),
		canRun,
		running,
		canCancel = false,
		oncancel,
		showStatementGutter = true,
		dirty = false,
		saving = false,
		catalogColumns = [],
		onchange,
		onactivity,
		onrun,
		showExplain = false,
		analyze = false,
		onanalyze,
		onexplain,
		showTransactions = false,
		manualCommit = false,
		transactionOpen = false,
		onmanualcommit,
		oncommit,
		onrollback,
		onsave,
		diagnostic = null,
		locateNonce = 0,
	}: Props = $props()

	let hostEl = $state<HTMLDivElement | null>(null)
	let view: EditorView | null = null
	let editorReady = $state(0)
	let scrolledKey = ''
	let mountedTab = ''
	let appliedLang = ''
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
		engine: 'sql',
		running: false,
	}
	$effect(() => {
		callbacks.onrun = onrun
		callbacks.onchange = onchange
		callbacks.onactivity = onactivity
		callbacks.engine = engine
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

	function statementGutter(): Extension {
		return gutter({
			class: 'cm-stmt-gutter',
			markers(view) {
				const doc = view.state.doc.toString()
				const spans = splitStatementSpans(
					doc,
					splitDialectFor(callbacks.engine),
				)
				const builder = new RangeSetBuilder<GutterMarker>()
				let lastLine = -1
				for (const span of spans) {
					const line = view.state.doc.lineAt(Math.min(span.start, doc.length))
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
		const dialect = splitDialectFor(engine)
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

	const isMac =
		typeof navigator !== 'undefined' &&
		/Mac|iPhone|iPad/.test(navigator.platform)
	const runShortcut = isMac ? '⌘Enter' : 'Ctrl+Enter'
	const formatShortcut = isMac ? '⌘⇧F' : 'Ctrl+Shift+F'
	const saveShortcut = isMac ? '⌘S' : 'Ctrl+S'

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

	function formatEditor() {
		const src = view?.state.doc.toString() ?? localText ?? ''
		const result = formatQuery(callbacks.engine, src)
		if (!result.ok) {
			showToast('danger', result.error)
			return
		}
		if (result.text === src || result.text === src + '\n') return
		applyTextToEditor(result.text)
		view?.focus()
	}

	/** Language mode, dialect and completions all come from the engine adapter. */
	function languageExtension(
		lang: string,
		eng: string,
		cols: CatalogColumnRow[],
	): Extension {
		const { language } = adapterFor(eng)
		if (lang === 'mql') {
			const autocomplete = language.completion(cols)
			return [
				javascript(),
				...(autocomplete ? [javascriptLanguage.data.of({ autocomplete })] : []),
			]
		}
		const dialect = language.sqlDialect ?? StandardSQL
		const complete = language.sqlSchema?.(cols)
		const extra = language.completion(cols)
		const support = sql({
			dialect,
			schema: (complete?.schema ?? {}) as SQLNamespace,
			defaultSchema: complete?.defaultSchema,
			upperCaseKeywords: true,
		})
		if (!extra) return support
		return [support, dialect.language.data.of({ autocomplete: extra })]
	}

	function buildExtensions(lang: string, ph: string): Extension[] {
		return [
			basicSetup,
			editorCompartments.runGutter.of(
				showStatementGutter ? [statementGutter()] : [],
			),
			mystiqueSearch(),
			editorCompartments.theme.of(createMystiqueEditorTheme(themeIsDark())),
			mystiqueHighlight,
			identPathHighlight,
			editorCompartments.lang.of(
				languageExtension(lang, engine, catalogColumns),
			),
			editorCompartments.placeholder.of(ph ? cmPlaceholder(ph) : []),
			autocompletion({ activateOnTyping: true, icons: true }),
			// No linter source. An empty source replaces setDiagnostics after its idle delay.
			EditorView.lineWrapping,
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
				if (update.docChanged || update.selectionSet) publishActivity()
			}),
		]
	}

	onMount(() => {
		if (!hostEl) return

		extensions = buildExtensions(language, placeholder)
		const existing = peekEditorState(tabId)
		if (existing) {
			view = new EditorView({ state: existing.state, parent: hostEl })
			view.scrollDOM.scrollTop = existing.scrollTop
			view.scrollDOM.scrollLeft = existing.scrollLeft
		} else {
			view = new EditorView({
				state: EditorState.create({
					doc: localText ?? draft ?? '',
					extensions,
				}),
				parent: hostEl,
			})
		}
		mountedTab = tabId
		editorReady += 1
		publishActivity()

		return () => {
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
		const show = showStatementGutter
		const v = view
		if (!v) return
		v.dispatch({
			effects: editorCompartments.runGutter.reconfigure(
				show ? [statementGutter()] : [],
			),
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
				effects: [
					editorCompartments.theme.reconfigure(
						createMystiqueEditorTheme(themeIsDark()),
					),
					editorCompartments.runGutter.reconfigure(
						showStatementGutter ? [statementGutter()] : [],
					),
				],
				annotations: Transaction.addToHistory.of(false),
			})
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
		const length = v.state.doc.length
		const from = range ? Math.min(range.from, length) : 0
		const to = range ? Math.min(Math.max(range.to, from), length) : 0
		const marks =
			range && to > from
				? [
						{
							from,
							to,
							severity: 'error' as const,
							message: range.message,
						},
					]
				: []
		v.dispatch(setDiagnostics(v.state, marks))
		const key = range && to > from ? `${nonce}:${from}:${to}` : ''
		if (!key) {
			scrolledKey = ''
			return
		}
		if (key === scrolledKey) return
		scrolledKey = key
		v.dispatch({
			selection: { anchor: from, head: to },
			scrollIntoView: true,
		})
		v.focus()
	})

	// Rebuild the SQL schema only when the catalog contents change.
	$effect(() => {
		const lang = language
		const ph = placeholder
		const eng = engine
		const cols = catalogColumns
		const id = tabId
		const sig = `${lang}\0${eng}\0${ph}\0${catalogSignature(cols)}`
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
	class="flex h-full min-h-0 min-w-0 flex-col"
	data-query-editor={connectionId}
>
	<label class="sr-only" for="query-cm-{connectionId}">Query editor</label>
	<div
		id="query-cm-{connectionId}"
		bind:this={hostEl}
		class="query-cm-host min-h-0 min-w-0 flex-1 overflow-hidden bg-input"
		role="textbox"
		aria-multiline="true"
		aria-label="Query editor"
	></div>

	<div
		class="flex shrink-0 items-center gap-2 border-t border-line bg-elevated px-2.5 py-1.5"
	>
		<div class="flex min-w-0 flex-1 items-center gap-2">
			<Pill uppercase>{language === 'mql' ? 'MQL' : 'SQL'}</Pill>
			<span class="font-mono text-caption tabular-nums text-fg-muted">
				{lineCount}
				{lineCount === 1 ? 'line' : 'lines'}
			</span>
			<span class="font-mono text-caption tabular-nums text-fg-muted">
				{localText.length.toLocaleString()} chars
			</span>
		</div>
		<div class="flex shrink-0 items-center gap-1.5">
			<Button
				variant="ghost"
				size="sm"
				class={appPrefs.agentChatOpen
					? 'border-accent/40 bg-accent-muted text-accent hover:border-accent/50 hover:bg-accent-muted'
					: ''}
				aria-pressed={appPrefs.agentChatOpen}
				onclick={() => appPrefs.setAgentChatOpen(!appPrefs.agentChatOpen)}
				title={appPrefs.agentChatOpen ? 'Close agent' : 'Open agent'}
			>
				<Icon name="chat" class="size-3.5" />
				Agent
			</Button>
			<span class="mx-0.5 h-4 w-px shrink-0 bg-line" aria-hidden="true"></span>
			{#if language === 'mql'}
				<Input
					class="w-32 font-mono"
					inputSize="sm"
					placeholder="collection"
					bind:value={collection}
					aria-label="Collection name"
					title="Collection name"
				/>
			{/if}
			{#if showTransactions}
				<Button
					variant="ghost"
					size="sm"
					aria-pressed={!manualCommit}
					disabled={running}
					onclick={() => onmanualcommit?.(!manualCommit)}
					title={manualCommit
						? 'Turn auto-commit on. An open transaction is rolled back.'
						: 'Turn auto-commit off and keep one connection for this tab.'}
				>
					Auto-commit
				</Button>
				{#if manualCommit}
					<span
						class="size-1.5 shrink-0 rounded-full {transactionOpen
							? 'bg-warning'
							: 'bg-fg-muted/40'}"
						title={transactionOpen ? 'Transaction open' : 'No open transaction'}
					></span>
					<Button
						variant="ghost"
						size="sm"
						disabled={running || !transactionOpen}
						onclick={() => oncommit?.()}
					>
						Commit
					</Button>
					<Button
						variant="ghost"
						size="sm"
						disabled={running || !transactionOpen}
						onclick={() => onrollback?.()}
					>
						Rollback
					</Button>
				{/if}
			{/if}
			{#if showExplain}
				<Button
					variant="ghost"
					size="sm"
					aria-pressed={analyze}
					disabled={running}
					onclick={() => onanalyze?.(!analyze)}
					title="Collect actual rows. A statement that is not a read still cannot change data."
				>
					Analyze
				</Button>
				<Button
					variant="ghost"
					size="sm"
					disabled={!localText.trim() || running}
					onclick={() => onexplain?.()}
					title="Explain the statement at the cursor"
				>
					Explain
				</Button>
			{/if}
			<Button
				variant="ghost"
				size="sm"
				disabled={!localText.trim() || running}
				onclick={formatEditor}
				title={`Format (${formatShortcut})`}
			>
				Format
			</Button>
			{#if onsave}
				<Button
					variant="ghost"
					size="sm"
					class={dirty && !saving
						? 'border-warning/50 bg-warning/10 text-warning hover:border-warning/60 hover:bg-warning/20'
						: ''}
					disabled={!localText.trim() || saving || running}
					onclick={onsave}
					title={dirty
						? `Unsaved changes (${saveShortcut})`
						: `Save (${saveShortcut})`}
				>
					{saving ? 'Saving…' : 'Save'}
				</Button>
			{/if}
			<Button
				variant="primary"
				size="sm"
				disabled={running ? !canCancel : !canRun}
				onclick={() => {
					if (running) oncancel?.()
					else emitRun(false)
				}}
				title={running && canCancel
					? 'Cancel (Esc)'
					: `Run statement (${runShortcut})`}
			>
				{running ? (canCancel ? 'Cancel' : 'Running…') : 'Run'}
			</Button>
		</div>
	</div>
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
		color: var(--text-muted);
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
		animation: mystique-search-in var(--duration-press) var(--ease-out) both;
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
