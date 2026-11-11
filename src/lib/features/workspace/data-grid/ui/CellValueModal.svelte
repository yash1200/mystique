<script module lang="ts">
	import { lazy } from '#core/ui'

	/** The hex view loads with the first binary cell opened. */
	const BinaryView = lazy(() => import('./BinaryView.svelte'))
</script>

<script lang="ts">
	import { errorMessage } from '#core/ipc'
	import { Button, Kbd, Lazy, Modal, Textarea } from '#core/ui'
	import { cn, copyTextToClipboard } from '#core/util'
	import { onMount, untrack } from 'svelte'
	import {
		CELL_VIEW_MODES,
		detectCellViewMode,
		jsonParseError,
		minifyJson,
		prettyJson,
	} from '../cell-view'
	import {
		PARTIAL_COPY_MESSAGE,
		UNKEYED_LARGE_CELL,
		type LargeCellPage,
	} from '../large-cell'
	import type { CellViewMode } from '../cell-view'
	import { isBinaryValue } from '../binary'
	import JsonHighlightEditor from './JsonHighlightEditor.svelte'

	interface Props {
		column: string
		columnType?: string
		rowIndex: number
		initialText: string
		/** When true, the value cannot be edited (query results). */
		readonly?: boolean
		/** Load the next chunk of a large cell. Absent when the preview is the whole value. */
		fetchChunk?: (offset: number) => Promise<LargeCellPage>
		/** Large cell with no primary key. The preview is all we can show. */
		unfetchable?: boolean
		/** Why it cannot load (defaults to the table-grid wording). */
		unfetchableText?: string
		onapply?: (text: string) => void
		onclose: () => void
	}

	let {
		column,
		columnType = '',
		rowIndex,
		initialText,
		readonly = false,
		fetchChunk,
		unfetchable = false,
		unfetchableText = UNKEYED_LARGE_CELL,
		onapply,
		onclose,
	}: Props = $props()

	// Modal remounts on each open; one-time seed (props are frozen via untrack).
	const openedMode = untrack(() => detectCellViewMode(initialText, columnType))
	let mode = $state<CellViewMode>(openedMode)
	let text = $state(
		untrack(() =>
			openedMode === 'json'
				? (prettyJson(initialText) ?? initialText)
				: initialText,
		),
	)
	let editorWrap = $state<HTMLDivElement | null>(null)
	let loading = $state(false)
	let loadedChars = $state(0)
	let totalChars = $state(0)
	let loadError = $state('')
	let copyNote = $state('')
	const previewOnly = untrack(() => unfetchable || !!fetchChunk)
	/** Bytes (bytea, BLOB): opens on the hex view. */
	const binary = untrack(
		() => initialText !== '' && initialText !== 'NULL' && isBinaryValue(initialText, columnType),
	)
	let hexView = $state(untrack(() => binary))
	const hexPartial = $derived(
		previewOnly && (loading || unfetchable || !!loadError || text === initialText),
	)
	const locked = $derived(readonly || loading || unfetchable || !!loadError)

	function focusEditor() {
		editorWrap?.querySelector('textarea')?.focus()
	}

	const jsonError = $derived(mode === 'json' ? jsonParseError(text) : null)
	const canApply = $derived(jsonError == null)

	function setMode(next: CellViewMode) {
		hexView = false
		if (next === mode) return
		if (next === 'json') {
			const pretty = prettyJson(text)
			if (pretty) text = pretty
		} else if (mode === 'json') {
			const compact = minifyJson(text)
			if (compact != null) text = compact
		}
		mode = next
		queueMicrotask(focusEditor)
	}

	function formatJson() {
		const pretty = prettyJson(text)
		if (pretty) text = pretty
	}

	async function loadFull() {
		const load = fetchChunk
		if (!load) return
		loading = true
		loadError = ''
		let offset = 0
		const parts: string[] = []
		try {
			for (;;) {
				const page = await load(offset)
				if (!alive) return
				parts.push(page.text)
				totalChars = page.totalChars
				loadedChars = page.nextOffset ?? page.totalChars
				if (page.nextOffset == null) break
				offset = page.nextOffset
			}
			if (!alive) return
			const full = parts.join('')
			text = mode === 'json' ? (prettyJson(full) ?? full) : full
		} catch (err) {
			if (!alive) return
			loadError = errorMessage(err)
		} finally {
			if (alive) loading = false
		}
	}

	async function copyValue() {
		const partial = loading || unfetchable || !!loadError || !!fetchChunk && text === initialText
		const payload = partial ? initialText : text
		const res = await copyTextToClipboard(payload)
		copyNote = !res.ok
			? 'Copy failed'
			: partial
				? PARTIAL_COPY_MESSAGE
				: 'Copied the full value.'
	}

	function apply() {
		if (locked || !canApply) return
		if (mode === 'json') {
			const compact = minifyJson(text)
			onapply?.(compact ?? text)
			return
		}
		onapply?.(text)
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Backspace' || e.key === 'Delete') {
			e.stopPropagation()
			if (readonly) e.preventDefault()
		}
		if (locked) return
		if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
			e.preventDefault()
			e.stopPropagation()
			apply()
		}
	}

	let alive = true

	onMount(() => {
		queueMicrotask(focusEditor)
		if (fetchChunk) void loadFull()
		window.addEventListener('keydown', onKeydown, true)
		return () => {
			alive = false
			window.removeEventListener('keydown', onKeydown, true)
		}
	})
</script>

<Modal
	open
	{onclose}
	layout="chrome"
	size="lg"
	height="md"
	closeButton={false}
	labelledby="cell-value-title"
>
	{#snippet header()}
		<div class="flex min-w-0 items-start justify-between gap-3">
			<div class="min-w-0">
				<h2
					id="cell-value-title"
					class="m-0 truncate font-mono text-body font-semibold text-fg"
				>
					{column}
				</h2>
				<p class="mt-0.5 font-mono text-caption text-fg-muted">
					Row {rowIndex + 1}{#if columnType}
						<span aria-hidden="true"> · </span>{columnType}{/if}
				</p>
			</div>
			<div
				class="flex shrink-0 rounded-md border border-line p-0.5"
				role="tablist"
				aria-label="Value format"
			>
				{#if binary}
					<button
						type="button"
						role="tab"
						aria-selected={hexView}
						class={cn(
							'h-control-sm rounded-sm px-2.5 text-label font-medium transition-colors',
							hexView
								? 'bg-accent-soft text-accent-text'
								: 'bg-transparent text-fg-muted hover:bg-hover hover:text-fg',
						)}
						onclick={() => (hexView = true)}
					>
						Hex
					</button>
				{/if}
				{#each CELL_VIEW_MODES as item (item.id)}
					<button
						type="button"
						role="tab"
						aria-selected={!hexView && mode === item.id}
						class={cn(
							'h-control-sm rounded-sm px-2.5 text-label font-medium transition-colors',
							!hexView && mode === item.id
								? 'bg-accent-soft text-accent-text'
								: 'bg-transparent text-fg-muted hover:bg-hover hover:text-fg',
						)}
						onclick={() => setMode(item.id)}
					>
						{item.label}
					</button>
				{/each}
			</div>
		</div>
	{/snippet}

	<div bind:this={editorWrap} class="flex min-h-0 flex-1 flex-col gap-2 p-3">
		{#if unfetchable}
			<p class="m-0 text-caption text-fg-muted">{unfetchableText}</p>
		{:else if loading}
			<p class="m-0 text-caption text-fg-muted">
				Loading {loadedChars.toLocaleString()} / {totalChars.toLocaleString()} characters
			</p>
		{:else if loadError}
			<p class="m-0 text-caption text-danger">{loadError}</p>
		{:else if copyNote}
			<p class="m-0 text-caption text-fg-muted">{copyNote}</p>
		{/if}
		{#if hexView}
			<Lazy
				component={BinaryView}
				{text}
				partial={hexPartial}
				totalBytes={hexPartial && totalChars > 2 && initialText.startsWith('\\x')
					? Math.floor((totalChars - 2) / 2)
					: null}
			/>
		{:else if mode === 'html'}
			<div
				class="grid min-h-0 flex-1 grid-rows-2 gap-2 md:grid-cols-2 md:grid-rows-1"
			>
				<label class="flex min-h-0 min-w-0 flex-col gap-1">
					<span class="text-caption font-medium text-fg-secondary">Source</span>
					<Textarea
						bind:value={text}
						spellcheck={false}
						autofocus
						readonly={locked}
						class="min-h-40 flex-1 resize-none font-mono text-label leading-relaxed"
					/>
				</label>
				<div class="flex min-h-0 min-w-0 flex-col gap-1">
					<span class="text-caption font-medium text-fg-secondary">Preview</span
					>
					<div
						class="min-h-40 flex-1 overflow-hidden rounded-md border border-line bg-app"
					>
						<iframe
							title="HTML preview"
							sandbox=""
							srcdoc={text}
							class="h-full min-h-40 w-full border-0 bg-app"
						></iframe>
					</div>
				</div>
			</div>
		{:else if mode === 'json'}
			<JsonHighlightEditor bind:value={text} readonly={locked} invalid={!!jsonError} />
		{:else}
			<Textarea
				bind:value={text}
				spellcheck={false}
				autofocus
				readonly={locked}
				class="min-h-64 flex-1 resize-none font-mono text-label leading-relaxed"
			/>
		{/if}
		{#if jsonError}
			<p class="mb-0 text-caption text-danger">{jsonError}</p>
		{/if}
	</div>

	{#snippet footer()}
		<div class="flex w-full items-center justify-between gap-2">
			{#if locked}
				<p class="m-0 flex items-center gap-1.5 text-caption text-fg-muted">
					<Kbd>esc</Kbd>
					<span>close</span>
				</p>
			{:else}
				<p class="m-0 flex items-center gap-1.5 text-caption text-fg-muted">
					<Kbd>⌘</Kbd><Kbd>↵</Kbd>
					<span>apply</span>
					<span aria-hidden="true">·</span>
					<Kbd>esc</Kbd>
					<span>cancel</span>
				</p>
			{/if}
			<div class="flex items-center gap-2">
				<Button variant="ghost" size="sm" onclick={() => void copyValue()}>
					{previewOnly && (loading || unfetchable || loadError) ? 'Copy preview' : 'Copy'}
				</Button>
				{#if mode === 'json' && !hexView}
					<Button
						variant="ghost"
						size="sm"
						onclick={formatJson}
						disabled={!!jsonError || !text.trim()}
					>
						Prettify
					</Button>
				{/if}
				{#if locked}
					<Button variant="primary" size="sm" onclick={onclose}>Close</Button>
				{:else}
					<Button variant="ghost" size="sm" onclick={onclose}>Cancel</Button>
					<Button
						variant="primary"
						size="sm"
						onclick={apply}
						disabled={!canApply}
					>
						Apply
					</Button>
				{/if}
			</div>
		</div>
	{/snippet}
</Modal>
