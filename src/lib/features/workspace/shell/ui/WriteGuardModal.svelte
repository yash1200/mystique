<script lang="ts">
	import type { WriteRowPreview } from '#core/contracts'
	import { errorMessage, previewWriteRows } from '#core/ipc'
	import { Button, Field, Input, Modal } from '#core/ui'
	import { cn } from '#core/util'
	import {
		guardCopy,
		isNewColumn,
		previewCell,
		previewIsEmpty,
		previewSummary,
		typedNameOk,
		currentWriteGuard,
		resolveWriteGuard,
		subscribeWriteGuard,
	} from '#features/workspace/session'
	import { onMount } from 'svelte'

	let prompt = $state(currentWriteGuard())
	let typed = $state('')
	/** Statement the row preview is for, and its result. */
	let previewIndex = $state(0)
	let preview = $state<WriteRowPreview | null>(null)
	let previewBusy = $state(false)
	let previewError = $state<string | null>(null)
	let previewRun = 0

	onMount(() =>
		subscribeWriteGuard(() => {
			prompt = currentWriteGuard()
			typed = ''
			resetPreview(0)
		}),
	)

	const copy = $derived(prompt ? guardCopy(prompt) : null)
	const typeOk = $derived(!prompt || typedNameOk(prompt, typed))
	const statements = $derived(prompt?.statements ?? [])
	/** On demand only, for production writes the backend sent SQL for. */
	const canPreview = $derived(
		!!prompt?.sessionId &&
			prompt.environment.trim().toLowerCase() === 'production' &&
			statements.length > 0,
	)

	function resetPreview(index: number) {
		previewRun += 1
		previewIndex = index
		preview = null
		previewBusy = false
		previewError = null
	}

	async function runPreview() {
		const sessionId = prompt?.sessionId
		const statement = statements[previewIndex]
		if (!sessionId || !statement) return
		const run = ++previewRun
		previewBusy = true
		previewError = null
		try {
			const result = await previewWriteRows({ sessionId, statement, database: null })
			if (run === previewRun) preview = result
		} catch (e) {
			if (run === previewRun) previewError = errorMessage(e)
		} finally {
			if (run === previewRun) previewBusy = false
		}
	}

	function cancel() {
		resolveWriteGuard({ ok: false })
	}

	function confirm() {
		if (!prompt || !typeOk) return
		resolveWriteGuard(
			copy?.typed ? { ok: true, typedName: typed.trim() } : { ok: true },
		)
	}
</script>

<Modal
	open={!!prompt}
	onclose={cancel}
	title={copy?.title ?? ''}
	description={copy?.description ?? ''}
	size="lg"
	dismissible
	onkeydown={(e) => {
		if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) confirm()
	}}
>
	{#if copy?.body}
		<pre
			class="mt-3 max-h-64 overflow-auto rounded-md border border-line bg-input px-3 py-2 font-mono text-caption leading-relaxed whitespace-pre-wrap text-fg"
		>{copy.body}</pre>
	{/if}
	{#if canPreview}
		<section class="mt-3" aria-label="Affected rows">
			<div class="flex flex-wrap items-center gap-2">
				<Button
					variant="ghost"
					size="sm"
					disabled={previewBusy}
					aria-busy={previewBusy || undefined}
					onclick={() => void runPreview()}
				>
					{previewBusy ? 'Reading rows…' : 'Preview affected rows'}
				</Button>
				{#if statements.length > 1}
					<span class="text-caption text-fg-muted">
						Statement {previewIndex + 1} of {statements.length}
					</span>
					<Button
						variant="ghost"
						size="sm"
						disabled={previewIndex === 0}
						onclick={() => resetPreview(previewIndex - 1)}
					>
						Previous
					</Button>
					<Button
						variant="ghost"
						size="sm"
						disabled={previewIndex >= statements.length - 1}
						onclick={() => resetPreview(previewIndex + 1)}
					>
						Next
					</Button>
				{/if}
			</div>
			{#if previewError}
				<p class="mt-2 mb-0 text-caption text-danger">
					Preview failed: {previewError}
				</p>
			{:else if preview}
				<p
					class={cn(
						'mt-2 mb-0 text-caption',
						!preview.available && 'text-fg-muted',
						preview.error && 'text-danger',
						previewIsEmpty(preview) && 'font-medium text-warning',
						preview.available && !preview.error && !previewIsEmpty(preview) && 'text-fg',
					)}
					aria-live="polite"
				>
					{previewSummary(preview)}
				</p>
				{#if preview.available && !preview.error && preview.rows.length > 0}
					<div
						class="mt-2 max-h-64 overflow-auto rounded-md border border-line"
					>
						<table class="w-full border-collapse font-mono text-caption">
							<thead class="sticky top-0 bg-panel">
								<tr>
									{#each preview.columns as column, i (i)}
										<th
											scope="col"
											class={cn(
												'border-b border-line px-2 py-1 text-left font-medium whitespace-nowrap text-fg-secondary',
												isNewColumn(preview, column.name) &&
													'bg-warning/10 text-warning',
											)}
										>
											{column.name}
										</th>
									{/each}
								</tr>
							</thead>
							<tbody>
								{#each preview.rows as row, r (r)}
									<tr class="border-b border-line last:border-b-0">
										{#each row as cell, c (c)}
											<td
												class={cn(
													'max-w-64 truncate px-2 py-1 whitespace-nowrap text-fg',
													cell === null && 'text-fg-muted italic',
													isNewColumn(preview, preview.columns[c]?.name ?? '') &&
														'bg-warning/10',
												)}
												title={previewCell(cell)}
											>
												{previewCell(cell)}
											</td>
										{/each}
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			{/if}
		</section>
	{/if}
	{#if copy?.typed}
		<div class="mt-3">
			<Field label="Object name" full>
				<Input bind:value={typed} autocomplete="off" />
			</Field>
		</div>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={cancel}>Cancel</Button>
		<Button
			variant={prompt?.kind === 'review' &&
			prompt.environment.toLowerCase() !== 'production'
				? 'primary'
				: 'danger'}
			size="sm"
			disabled={!typeOk}
			onclick={confirm}
		>
			{copy?.confirm ?? 'Run'}
		</Button>
	{/snippet}
</Modal>
