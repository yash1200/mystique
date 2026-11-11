<script lang="ts">
	import type {
		ImportColumnMap,
		ImportEncoding,
		ImportFormat,
		ImportOnDuplicate,
		PreviewImportResult,
	} from '#core/contracts'
	import { Alert, Button, Field, Icon, Input, Modal, Pill, Select } from '#core/ui'
	import { cn, inferFormatFromPath, pickDataFile } from '#core/util'
	import type { SelectOption } from '#features/workspace/data-grid'
	import { importDialogCopy } from '../import-dialog'

	export type ImportFileSpec = {
		path: string
		format: ImportFormat
	}

	export type ImportRunSpec = {
		mapping: ImportColumnMap[]
		encoding: ImportEncoding
		onDuplicate: ImportOnDuplicate
	}

	const FORMAT_LABELS: Record<string, string> = {
		csv: 'CSV',
		json: 'JSON',
		jsonl: 'JSON Lines',
		sql: 'SQL',
	}

	const ENCODING_OPTIONS: SelectOption[] = [
		{ value: 'utf8', label: 'UTF-8' },
		{ value: 'utf16Le', label: 'UTF-16 LE' },
		{ value: 'utf16Be', label: 'UTF-16 BE' },
		{ value: 'windows1252', label: 'Windows-1252' },
		{ value: 'iso88591', label: 'ISO-8859-1' },
	]

	const MODE_OPTIONS: SelectOption[] = [
		{ value: 'stopOnError', label: 'Stop on error' },
		{ value: 'updateOnDuplicate', label: 'Update on duplicate' },
	]

	interface Props {
		open: boolean
		objectName: string
		objectLabel?: string
		/** The connection it writes to, for the footer. */
		connectionName?: string
		formats?: string[]
		/** Label the run "Import into production". */
		production?: boolean
		preview: PreviewImportResult | null
		busy?: boolean
		error?: string | null
		/** A file was picked: load its preview. */
		onfile: (spec: ImportFileSpec) => void
		onencoding: (encoding: ImportEncoding) => void
		onconfirm: (spec: ImportRunSpec) => void
		oncancel: () => void
	}

	let {
		open,
		objectName,
		objectLabel = 'table',
		connectionName,
		formats = ['csv', 'json', 'jsonl'],
		production = false,
		preview,
		busy = false,
		error = null,
		onfile,
		onencoding,
		onconfirm,
		oncancel,
	}: Props = $props()

	let path = $state('')
	let format = $state<ImportFormat | null>(null)
	let picking = $state(false)
	let wasOpen = $state(false)
	let mapping = $state<ImportColumnMap[]>([])
	let encoding = $state<ImportEncoding>('utf8')
	let onDuplicate = $state<ImportOnDuplicate>('stopOnError')
	let seededKeys = $state('')

	const fileLabel = $derived(path.split(/[/\\]/).pop() || path)
	const detectedLabel = $derived(
		format ? (FORMAT_LABELS[format] ?? format) : null,
	)
	const copy = $derived(
		importDialogCopy({ objectName, production, busy, hasPreview: !!preview }),
	)

	const columnOptions = $derived.by((): SelectOption[] => {
		const cols = preview?.columns ?? []
		return [
			{ value: '', label: '— skip —' },
			...cols.map((c) => ({
				value: c.name,
				label: c.pk ? `${c.name} (PK)` : c.name,
			})),
		]
	})

	const mappedCount = $derived(
		mapping.filter((m) => (m.tableColumn ?? '').trim()).length,
	)

	$effect(() => {
		if (open && !wasOpen) {
			path = ''
			format = null
			seededKeys = ''
		}
		wasOpen = open
	})

	$effect(() => {
		if (!open || !preview) return
		const key = `${preview.fileKeys.join('\0')}|${preview.encoding}`
		if (seededKeys === key) return
		seededKeys = key
		mapping = preview.suggestedMap.map((m) => ({
			fileKey: m.fileKey,
			tableColumn: m.tableColumn ?? '',
		}))
		encoding = preview.encoding
		onDuplicate = 'stopOnError'
	})

	async function browseFile() {
		if (busy || picking) return
		picking = true
		try {
			const picked = await pickDataFile(formats, path || null)
			if (!picked) return
			path = picked
			format = inferFormatFromPath(picked)
			if (format) onfile({ path: picked, format })
		} finally {
			picking = false
		}
	}

	function mappedValue(row: unknown[], fileKey: string): unknown {
		const idx = preview?.fileKeys.indexOf(fileKey) ?? -1
		return idx >= 0 ? (row[idx] ?? null) : null
	}

	function displayCell(v: unknown): string {
		if (v == null) return ''
		if (typeof v === 'string') return v
		if (typeof v === 'number' || typeof v === 'boolean') return String(v)
		try {
			return JSON.stringify(v)
		} catch {
			return String(v)
		}
	}

	function setMap(fileKey: string, tableColumn: string) {
		mapping = mapping.map((m) =>
			m.fileKey === fileKey ? { ...m, tableColumn } : m,
		)
	}

	function submit() {
		if (busy || !preview || !mappedCount) return
		onconfirm({
			mapping: mapping.map((m) => ({
				fileKey: m.fileKey,
				tableColumn: (m.tableColumn ?? '').trim() || null,
			})),
			encoding,
			onDuplicate,
		})
	}
</script>

{#snippet step(n: number, label: string, state: 'done' | 'on' | 'todo')}
	<span class={cn('flex items-center gap-2 text-label', state === 'todo' ? 'text-fg-muted' : 'text-fg')}>
		<b
			class={cn(
				'grid size-5 place-items-center rounded-full text-caption font-semibold [&_svg]:size-[0.6875rem]',
				state === 'on' && 'bg-accent text-on-accent',
				state === 'done' && 'bg-accent-soft text-accent-text',
				state === 'todo' && 'bg-hover text-fg-secondary',
			)}
		>
			{#if state === 'done'}<Icon name="check" />{:else}{n}{/if}
		</b>
		{label}
	</span>
{/snippet}

<Modal
	{open}
	onclose={oncancel}
	icon="upload"
	description={preview
		? `Showing the first ${preview.rows.length} row${preview.rows.length === 1 ? '' : 's'} of ${fileLabel}.`
		: `Choose a file to load into this ${objectLabel}.`}
	note={!preview
		? undefined
		: mappedCount === 0
			? 'Map at least one column to continue'
			: connectionName
				? `Writes to ${connectionName}`
				: undefined}
	size="lg"
	height={preview ? 'lg' : 'auto'}
	dismissible={!busy}
>
	{#snippet titleContent()}
		Import into <span class="font-mono font-medium">{objectName}</span>
	{/snippet}
	{#snippet titleAside()}
		{#if production}<Pill tone="warning" uppercase>Prod</Pill>{/if}
		<span class="ml-auto flex items-center gap-2 pl-2">
			{@render step(1, 'Choose file', preview ? 'done' : 'on')}
			<span class="h-px w-5 bg-line-strong" aria-hidden="true"></span>
			{@render step(2, 'Map columns', preview ? 'on' : 'todo')}
		</span>
	{/snippet}

	<Field label="File" htmlFor="import-data-file">
		<div class="flex min-w-0 gap-2">
			<Input
				id="import-data-file"
				class="min-w-0 flex-1 font-mono text-data"
				value={fileLabel}
				disabled
				placeholder="No file selected"
			/>
			<Button class="shrink-0" disabled={busy || picking} onclick={() => void browseFile()}>
				{path ? 'Change…' : 'Browse…'}
			</Button>
		</div>
		{#if path && detectedLabel}
			<span class="text-label text-fg-muted">Detected as {detectedLabel}</span>
		{:else if path && !format}
			<span class="text-label text-danger">
				Unrecognized file type. Use {formats.map((f) => FORMAT_LABELS[f] ?? f).join(', ')}.
			</span>
		{/if}
	</Field>

	{#if preview}
		<div class="grid grid-cols-1 gap-3.5 sm:grid-cols-2">
			<Field label="Encoding" htmlFor="import-preview-encoding">
				<Select
					id="import-preview-encoding"
					value={encoding}
					options={ENCODING_OPTIONS}
					inputSize="sm"
					disabled={busy}
					onchange={(v) => {
						encoding = v as ImportEncoding
						onencoding(encoding)
					}}
				/>
			</Field>
			<Field label="On insert error" htmlFor="import-preview-mode">
				<Select
					id="import-preview-mode"
					value={onDuplicate}
					options={MODE_OPTIONS}
					inputSize="sm"
					disabled={busy}
					onchange={(v) => (onDuplicate = v as ImportOnDuplicate)}
				/>
			</Field>
		</div>

		{#if mappedCount === 0}
			<Alert tone="warning">
				None of the file’s fields match a column in <span class="font-mono">{objectName}</span> by
				name. Check you picked the right {objectLabel}, or map them by hand.
			</Alert>
		{:else}
			<Alert tone="success">
				{mappedCount} of {mapping.length} fields mapped · skipped fields are ignored
			</Alert>
		{/if}

		<div class="overflow-auto rounded-[10px] shadow-[inset_0_0_0_1px_var(--line)]">
			<table class="min-w-full border-collapse font-mono text-label">
				<thead>
					<tr class="bg-surface">
						{#each mapping as map (map.fileKey)}
							{@const mapped = !!(map.tableColumn ?? '').trim()}
							<th class="min-w-40 px-2 py-2 align-bottom font-normal">
								<Select
									value={map.tableColumn ?? ''}
									options={columnOptions}
									inputSize="sm"
									mono
									tint={mapped ? 'accent' : 'muted'}
									ariaLabel="Map {map.fileKey}"
									disabled={busy}
									onchange={(v) => setMap(map.fileKey, v)}
								/>
							</th>
						{/each}
					</tr>
					<tr class="border-t border-line bg-surface text-left">
						{#each mapping as map (map.fileKey)}
							<th
								class={cn(
									'h-[2.125rem] px-3 text-[0.71875rem] font-medium whitespace-nowrap text-fg-secondary',
									!(map.tableColumn ?? '').trim() && 'opacity-45',
								)}
								title={map.fileKey}>{map.fileKey}</th
							>
						{/each}
					</tr>
				</thead>
				<tbody>
					{#each preview.rows as row, ri (ri)}
						<tr class="border-t border-line">
							{#each mapping as map (map.fileKey)}
								<td
									class={cn(
										'h-[1.875rem] max-w-48 truncate px-3 text-fg',
										!(map.tableColumn ?? '').trim() && 'opacity-45',
									)}
									title={displayCell(mappedValue(row, map.fileKey))}
									>{displayCell(mappedValue(row, map.fileKey))}</td
								>
							{/each}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}

	{#if error}
		<p class="m-0 text-label text-danger" role="alert">{error}</p>
	{/if}

	{#snippet footer()}
		<Button disabled={busy} onclick={oncancel}>Cancel</Button>
		<Button
			variant={production ? 'danger' : 'primary'}
			disabled={busy || !preview || mappedCount === 0}
			onclick={submit}
		>
			{copy.confirm}
		</Button>
	{/snippet}
</Modal>
