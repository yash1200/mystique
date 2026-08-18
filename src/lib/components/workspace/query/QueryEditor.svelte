<script lang="ts">
	/**
	 * Query editor with CodeMirror 6 syntax highlighting (SQL / MQL).
	 * Highlighting is frontend-only — live as you type, no backend.
	 */
	import type { CatalogColumnRow } from '$lib/api/engines'
	import { Button, Input } from '$lib/components/ui'
	import { watchHtmlAttr } from '$lib/utils/appPrefs'
	import { formatQuery } from '$lib/utils/formatQuery'
	import {
		mongoCompletionSource,
		sqlCompleteFromColumns,
		sqlDialectForEngine,
	} from '$lib/utils/queryComplete'
	import { autocompletion } from '@codemirror/autocomplete'
	import { indentWithTab } from '@codemirror/commands'
	import { javascript, javascriptLanguage } from '@codemirror/lang-javascript'
	import { sql } from '@codemirror/lang-sql'
	import {
		Compartment,
		EditorState,
		Prec,
		type Extension,
	} from '@codemirror/state'
	import {
		placeholder as cmPlaceholder,
		EditorView,
		keymap,
	} from '@codemirror/view'
	import { basicSetup } from 'codemirror'
	import { onMount } from 'svelte'
	import {
		createMystiqueEditorTheme,
		identPathHighlight,
		mystiqueHighlight,
	} from './cmTheme'

	interface Props {
		connectionId: string
		/** Engine id for dialect-aware formatting (postgres, mysql, sqlite, mongodb, …). */
		engine: string
		placeholder: string
		localText?: string
		lineCount: number
		language: string
		collection?: string
		canRun: boolean
		running: boolean
		/** True when draft differs from last saved baseline. */
		dirty?: boolean
		/** True while a save is in flight. */
		saving?: boolean
		/** Live catalog columns for schema/table/field completions. */
		catalogColumns?: CatalogColumnRow[]
		/** Fired when document text changes. */
		onchange?: (text: string) => void
		onrun: () => void
		/** Save query for this connection (name prompt handled by parent). */
		onsave?: () => void
	}

	let {
		connectionId,
		engine,
		placeholder,
		localText = $bindable(''),
		lineCount,
		language,
		collection = $bindable(''),
		canRun,
		running,
		dirty = false,
		saving = false,
		catalogColumns = [],
		onchange,
		onrun,
		onsave,
	}: Props = $props()

	let hostEl = $state<HTMLDivElement | null>(null)
	let view: EditorView | null = null
	let langCompartment = new Compartment()
	let placeholderCompartment = new Compartment()
	let themeCompartment = new Compartment()

	function themeIsDark(): boolean {
		if (typeof document === 'undefined') return true
		return document.documentElement.dataset.theme !== 'light'
	}
	/** Skip echo when we push parent → editor. */
	let applyingExternal = false

	// Stable bag so CM keymaps/listeners always call latest props.
	const callbacks = {
		onrun: () => {},
		onformat: () => {},
		onsave: undefined as Props['onsave'],
		onchange: undefined as Props['onchange'],
		engine: 'sql',
	}
	$effect(() => {
		callbacks.onrun = onrun
		callbacks.onchange = onchange
		callbacks.engine = engine
		callbacks.onformat = formatEditor
		callbacks.onsave = onsave
	})

	let formatMsg = $state<string | null>(null)
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
			formatMsg = result.error
			setTimeout(() => {
				if (formatMsg === result.error) formatMsg = null
			}, 2800)
			return
		}
		if (result.text === src || result.text === src + '\n') {
			formatMsg = 'Already formatted'
			setTimeout(() => {
				if (formatMsg === 'Already formatted') formatMsg = null
			}, 1600)
			return
		}
		applyTextToEditor(result.text)
		formatMsg = null
		view?.focus()
	}

	function languageExtension(
		lang: string,
		eng: string,
		cols: CatalogColumnRow[],
	): Extension {
		if (lang === 'mql') {
			return [
				javascript(),
				javascriptLanguage.data.of({
					autocomplete: mongoCompletionSource(cols),
				}),
			]
		}
		const complete = sqlCompleteFromColumns(eng, cols)
		return sql({
			dialect: sqlDialectForEngine(eng),
			schema: complete.schema,
			defaultSchema: complete.defaultSchema,
		})
	}

	function buildExtensions(lang: string, ph: string): Extension[] {
		return [
			basicSetup,
			themeCompartment.of(createMystiqueEditorTheme(themeIsDark())),
			mystiqueHighlight,
			identPathHighlight,
			langCompartment.of(languageExtension(lang, engine, catalogColumns)),
			placeholderCompartment.of(ph ? cmPlaceholder(ph) : []),
			autocompletion({ activateOnTyping: true, icons: true }),
			EditorView.lineWrapping,
			// Beat basicSetup's defaultKeymap (Mod-Enter = insertBlankLine).
			Prec.highest(
				keymap.of([
					{
						key: 'Mod-Enter',
						run: () => {
							callbacks.onrun()
							return true
						},
					},
					{
						key: 'Ctrl-Enter',
						run: () => {
							callbacks.onrun()
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
				if (!update.docChanged || applyingExternal) return
				const next = update.state.doc.toString()
				localText = next
				callbacks.onchange?.(next)
			}),
		]
	}

	onMount(() => {
		if (!hostEl) return

		const state = EditorState.create({
			doc: localText ?? '',
			extensions: buildExtensions(language, placeholder),
		})

		view = new EditorView({
			state,
			parent: hostEl,
		})

		const unwatchTheme = watchHtmlAttr('data-theme', () => {
			const v = view
			if (!v) return
			v.dispatch({
				effects: themeCompartment.reconfigure(
					createMystiqueEditorTheme(themeIsDark()),
				),
			})
		})

		return () => {
			unwatchTheme()
			view?.destroy()
			view = null
		}
	})

	// Parent draft / history → editor (never while user is typing in the CM DOM).
	$effect(() => {
		const next = localText ?? ''
		const v = view
		if (!v) return
		const cur = v.state.doc.toString()
		if (cur === next) return
		if (v.hasFocus) return
		applyingExternal = true
		v.dispatch({
			changes: { from: 0, to: cur.length, insert: next },
		})
		applyingExternal = false
	})

	// Language / placeholder / catalog completions when engine or schema changes.
	$effect(() => {
		const lang = language
		const ph = placeholder
		const eng = engine
		const cols = catalogColumns
		const v = view
		if (!v) return
		v.dispatch({
			effects: [
				langCompartment.reconfigure(languageExtension(lang, eng, cols)),
				// CodeMirror treats empty string as no placeholder when extension is still present.
				placeholderCompartment.reconfigure(ph ? cmPlaceholder(ph) : []),
			],
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
		class="flex h-control-sm shrink-0 items-center gap-2 border-t border-line bg-elevated px-2.5"
	>
		<span
			class="min-w-0 flex-1 truncate font-mono text-caption tabular-nums text-fg-muted"
		>
			{lineCount} line{lineCount === 1 ? '' : 's'}
			· {localText.length} chars
			{#if formatMsg}
				<span class="ml-1.5 text-warning">· {formatMsg}</span>
			{/if}
		</span>
		<div class="flex shrink-0 items-center gap-1.5">
			{#if language === 'mql'}
				<Input
					class="w-32 font-mono"
					inputSize="sm"
					placeholder="collection…"
					bind:value={collection}
					title="Collection name"
				/>
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
				disabled={running || !canRun}
				onclick={onrun}
				title={`Run (${runShortcut})`}
			>
				{running ? 'Running…' : 'Run'}
			</Button>
		</div>
	</div>
</div>

<style>
	/* CodeMirror fills the host; basicSetup adds gutters we style via theme. */
	.query-cm-host :global(.cm-editor) {
		height: 100%;
	}
	.query-cm-host :global(.cm-scroller) {
		min-height: 100%;
	}
</style>
