<script lang="ts">
	import type { WriteRowPreview } from '#core/contracts'
	import { errorMessage, previewWriteRows } from '#core/ipc'
	import { Alert, Button, CodePreview, Input, Modal, Pill, type IconName } from '#core/ui'
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
	const production = $derived(prompt?.environment.trim().toLowerCase() === 'production')
	/** Drop / truncate get the bin; other writes the shield; unlocking the open lock. */
	const look = $derived.by((): { icon: IconName; tone: 'danger' | 'warning' | 'accent' } => {
		if (!prompt || !copy) return { icon: 'shield-alert', tone: 'danger' }
		if (/^(drop|truncate)\b/i.test(copy.title)) return { icon: 'trash', tone: 'danger' }
		if (prompt.kind === 'unlock') return { icon: 'lock-open', tone: 'warning' }
		if (prompt.kind === 'review') {
			return production ? { icon: 'shield-alert', tone: 'warning' } : { icon: 'check', tone: 'accent' }
		}
		return { icon: 'shield-alert', tone: 'danger' }
	})
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
	role="alertdialog"
	icon={look.icon}
	tone={look.tone}
	title={copy?.title ?? ''}
	description={copy?.description ?? ''}
	note={prompt ? `Runs on ${prompt.connectionName}` : undefined}
	size={canPreview ? 'lg' : 'md'}
	dismissible
	onkeydown={(e) => {
		if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) confirm()
	}}
>
	{#snippet titleAside()}
		{#if production}<Pill tone="warning" uppercase>Prod</Pill>{/if}
	{/snippet}
	{#if copy?.typed}
		<div class="flex flex-col gap-1.5">
			<label class="text-label font-medium text-fg-secondary" for="write-guard-name">
				Type <span class="font-mono text-fg">{prompt?.objectName}</span> to confirm
			</label>
			<Input
				id="write-guard-name"
				class="font-mono"
				bind:value={typed}
				placeholder={prompt?.objectName ?? ''}
				autofocus
			/>
		</div>
	{/if}
	{#if copy?.warnings.length}
		<Alert tone="warning">
			<ul class="m-0 list-none space-y-1 p-0">
				{#each copy.warnings as warning, i (i)}
					<li>{warning}</li>
				{/each}
			</ul>
		</Alert>
	{/if}
	{#if copy?.body}
		<CodePreview code={copy.body} lang={statements.length ? 'sql' : 'plain'} />
	{/if}
	{#if canPreview}
		<section aria-label="Affected rows">
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
					<div class="mt-2 max-h-64 overflow-auto rounded-[9px] border border-line">
						<table class="w-full border-collapse font-mono text-caption">
							<thead class="sticky top-0 bg-surface">
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
	{#snippet footer()}
		<Button onclick={cancel}>Cancel</Button>
		<Button
			variant={prompt?.kind === 'review' && !production ? 'primary' : 'danger'}
			disabled={!typeOk}
			onclick={confirm}
		>
			{copy?.confirm ?? 'Run'}
		</Button>
	{/snippet}
</Modal>
