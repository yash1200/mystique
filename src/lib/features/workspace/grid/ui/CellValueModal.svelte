<script lang="ts">
	import { Button, Kbd, Modal, Textarea } from '$core/ui'
	import { cn } from '$core/util'
	import { onMount, untrack } from 'svelte'
	import {
		CELL_VIEW_MODES,
		detectCellViewMode,
		jsonParseError,
		minifyJson,
		prettyJson,
	} from '../cell-view'
	import type { CellViewMode } from '../types'
	import JsonHighlightEditor from './JsonHighlightEditor.svelte'

	interface Props {
		column: string
		columnType?: string
		rowIndex: number
		initialText: string
		/** When true, the value cannot be edited (query results). */
		readonly?: boolean
		onapply?: (text: string) => void
		onclose: () => void
	}

	let {
		column,
		columnType = '',
		rowIndex,
		initialText,
		readonly = false,
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

	function focusEditor() {
		editorWrap?.querySelector('textarea')?.focus()
	}

	const jsonError = $derived(mode === 'json' ? jsonParseError(text) : null)
	const canApply = $derived(jsonError == null)

	function setMode(next: CellViewMode) {
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

	function apply() {
		if (readonly || !canApply) return
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
		if (readonly) return
		if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
			e.preventDefault()
			e.stopPropagation()
			apply()
		}
	}

	onMount(() => {
		queueMicrotask(focusEditor)
		window.addEventListener('keydown', onKeydown, true)
		return () => window.removeEventListener('keydown', onKeydown, true)
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
				{#each CELL_VIEW_MODES as item (item.id)}
					<button
						type="button"
						role="tab"
						aria-selected={mode === item.id}
						class={cn(
							'h-control-sm rounded-sm px-2.5 text-label font-medium transition-colors',
							mode === item.id
								? 'bg-accent-muted text-accent'
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
		{#if mode === 'html'}
			<div
				class="grid min-h-0 flex-1 grid-rows-2 gap-2 md:grid-cols-2 md:grid-rows-1"
			>
				<label class="flex min-h-0 min-w-0 flex-col gap-1">
					<span class="text-caption font-medium text-fg-secondary">Source</span>
					<Textarea
						bind:value={text}
						spellcheck={false}
						autofocus
						{readonly}
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
			<JsonHighlightEditor bind:value={text} {readonly} invalid={!!jsonError} />
		{:else}
			<Textarea
				bind:value={text}
				spellcheck={false}
				autofocus
				{readonly}
				class="min-h-64 flex-1 resize-none font-mono text-label leading-relaxed"
			/>
		{/if}
		{#if jsonError}
			<p class="mb-0 text-caption text-danger">{jsonError}</p>
		{/if}
	</div>

	{#snippet footer()}
		<div class="flex w-full items-center justify-between gap-2">
			{#if readonly}
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
				{#if mode === 'json'}
					<Button
						variant="ghost"
						size="sm"
						onclick={formatJson}
						disabled={!!jsonError || !text.trim()}
					>
						Prettify
					</Button>
				{/if}
				{#if readonly}
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
