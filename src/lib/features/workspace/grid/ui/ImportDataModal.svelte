<script lang="ts">
	import type {
		ImportColumnMap,
		ImportEncoding,
		ImportFormat,
		ImportOnDuplicate,
		PreviewImportResult,
	} from '#core/contracts'
	import { isTauri } from '#core/ipc'
	import { Button, Field, Input, Modal, Select } from '#core/ui'
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

	const desktop = isTauri()
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
		if (!desktop || busy || picking) return
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

<Modal
	{open}
	onclose={oncancel}
	title={copy.title}
	description={preview
		? `First ${preview.rows.length} row${preview.rows.length === 1 ? '' : 's'} from the file. Map each file field to a column.`
		: `Choose a file to load into this ${objectLabel}.`}
	layout="chrome"
	size="lg"
	height={preview ? 'lg' : 'auto'}
	closeButton={false}
	dismissible={!busy}
>
	<div class="min-h-0 flex-1 overflow-auto px-4 py-3">
		<Field label="File" htmlFor="import-data-file">
			<div class="flex min-w-0 items-center gap-1.5">
				<div class="min-w-0 flex-1">
					<Input
						id="import-data-file"
						value={fileLabel}
						inputSize="sm"
						disabled
						placeholder={desktop
							? 'No file selected'
							: 'Import requires the desktop app'}
					/>
				</div>
				<Button
					variant="secondary"
					size="sm"
					disabled={busy || picking || !desktop}
					onclick={() => void browseFile()}
				>
					Browse
				</Button>
			</div>
			{#if path && detectedLabel}
				<p class="mt-1 mb-0 text-caption text-fg-muted">
					Detected as {detectedLabel}
				</p>
			{:else if path && !format}
				<p class="mt-1 mb-0 text-caption text-danger">
					Unrecognized file type. Use {formats
						.map((f) => FORMAT_LABELS[f] ?? f)
						.join(', ')}.
				</p>
			{/if}
		</Field>

		{#if preview}
			<div class="mt-3 grid gap-3 sm:grid-cols-2">
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
				<Field label="On insert" htmlFor="import-preview-mode">
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

			<h3 class="mt-4 mb-2 text-caption font-medium text-fg-secondary">
				Mapping
			</h3>
			<div class="overflow-auto rounded-md border border-line">
				<table class="min-w-full border-collapse text-label">
					<thead>
						<tr class="bg-panel">
							{#each mapping as map (map.fileKey)}
								<th class="min-w-40 px-2 py-1.5 align-bottom font-medium">
									<Select
										value={map.tableColumn ?? ''}
										options={columnOptions}
										inputSize="sm"
										disabled={busy}
										onchange={(v) => setMap(map.fileKey, v)}
									/>
								</th>
							{/each}
						</tr>
						<tr class="border-t border-line bg-panel text-left text-fg-muted">
							{#each mapping as map (map.fileKey)}
								<th
									class="whitespace-nowrap px-2.5 py-1.5 font-mono font-medium"
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
											'max-w-48 truncate px-2.5 py-1 font-mono text-fg',
											!(map.tableColumn ?? '').trim() && 'text-fg-muted',
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
			<p class="mt-3 mb-0 text-caption text-danger" role="alert">{error}</p>
		{/if}
	</div>

	{#snippet footer()}
		<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
			Cancel
		</Button>
		<Button
			variant={production ? 'danger' : 'primary'}
			size="sm"
			disabled={busy || !preview || mappedCount === 0}
			onclick={submit}
		>
			{copy.confirm}
		</Button>
	{/snippet}
</Modal>
