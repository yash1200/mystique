<script lang="ts">
	import { Button, Checkbox, Field, Input, Modal, Select } from '#core/ui'
	import type { TableFileFormat } from '#core/util'
	import {
		applyFormatExtension,
		defaultDownloadsDir,
		extensionForFormat,
		pickDirectory,
		safeExportFileBase,
	} from '#core/util'
	import type { SelectOption } from '#features/workspace/data-grid'
	export type ExportDataSpec = {
		fileName: string
		folder: string
		format: TableFileFormat
		/** Read through the query tab's open transaction (`txnTab`). */
		throughTransaction?: boolean
	}

	const FORMAT_LABELS: Record<string, string> = {
		csv: 'CSV',
		json: 'JSON',
		jsonl: 'JSON Lines',
		sql: 'SQL',
	}

	interface Props {
		open: boolean
		objectName: string
		objectLabel?: string
		/** Plugin-owned format ids (`csv` / `json` / `jsonl` / `sql`). */
		formats?: string[]
		busy?: boolean
		error?: string | null
		/** A query tab on this connection with an open transaction. */
		txnTab?: { title: string } | null
		onconfirm: (spec: ExportDataSpec) => void
		oncancel: () => void
	}

	let {
		open,
		objectName,
		objectLabel = 'table',
		formats = ['csv', 'json', 'jsonl'],
		busy = false,
		error = null,
		txnTab = null,
		onconfirm,
		oncancel,
	}: Props = $props()

	let fileName = $state('')
	let folder = $state('')
	let format = $state<TableFileFormat>('csv')
	let folderBusy = $state(false)
	let throughTransaction = $state(true)

	const formatOptions = $derived.by((): SelectOption[] => {
		const ids = formats.length > 0 ? formats : ['csv']
		return ids.map((id) => ({
			value: id,
			label: FORMAT_LABELS[id] ?? id.toUpperCase(),
		}))
	})

	const previewName = $derived(applyFormatExtension(fileName, format))

	let openKey = $state(false)

	$effect(() => {
		if (!open) {
			openKey = false
			return
		}
		if (openKey) return
		openKey = true
		fileName = safeExportFileBase(objectName)
		format = 'csv'
		folder = ''
		throughTransaction = true
		void loadDownloads()
	})

	async function loadDownloads() {
		const dir = await defaultDownloadsDir()
		if (dir) folder = dir
	}

	async function browseFolder() {
		if (busy || folderBusy) return
		folderBusy = true
		try {
			const picked = await pickDirectory(folder || null)
			if (picked) folder = picked
		} finally {
			folderBusy = false
		}
	}

	function submit() {
		const name = fileName.trim()
		if (!name || busy) return
		if (!folder.trim()) return
		onconfirm({
			fileName: applyFormatExtension(name, format),
			folder: folder.trim(),
			format,
			throughTransaction: !!txnTab && throughTransaction,
		})
	}

	function onFormatChange(value: string) {
		format = value as TableFileFormat
	}
</script>

<Modal
	{open}
	onclose={oncancel}
	title="Export data"
	description="Write the full {objectLabel} to a file. It runs in the background; you can keep working."
	dismissible={!busy}
>
	<div class="mt-3 flex flex-col gap-3">
		<Field label="File name" htmlFor="export-data-name">
			<Input
				id="export-data-name"
				bind:value={fileName}
				inputSize="sm"
				disabled={busy}
				placeholder="export"
				autofocus
			/>
			<p class="mt-1 mb-0 text-caption text-fg-muted">
				Saves as {previewName}
			</p>
		</Field>

		<Field label="Folder" htmlFor="export-data-folder">
			<div class="flex min-w-0 items-center gap-1.5">
				<div class="min-w-0 flex-1">
					<Input
						id="export-data-folder"
						bind:value={folder}
						inputSize="sm"
						disabled={busy}
						placeholder="Choose a folder…"
					/>
				</div>
				<Button
					variant="secondary"
					size="sm"
					disabled={busy || folderBusy}
					onclick={() => void browseFolder()}
				>
					Browse
				</Button>
			</div>
		</Field>

		<Field label="Format" htmlFor="export-data-format">
			<Select
				id="export-data-format"
				value={format}
				options={formatOptions}
				inputSize="sm"
				disabled={busy}
				onchange={onFormatChange}
			/>
		</Field>

		{#if txnTab}
			<div class="flex flex-col gap-1">
				<Checkbox
					bind:checked={throughTransaction}
					disabled={busy}
					label="Include uncommitted changes from “{txnTab.title}”"
				/>
				<p class="m-0 text-caption text-fg-muted">
					{throughTransaction
						? 'Reads through that tab’s open transaction, as the tab sees the data. The tab waits while the export runs.'
						: 'Reads committed data only.'}
				</p>
			</div>
		{/if}

		{#if error}
			<p class="mb-0 text-caption text-danger" role="alert">{error}</p>
		{/if}
	</div>

	{#snippet footer()}
		<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
			Cancel
		</Button>
		<Button
			variant="primary"
			size="sm"
			disabled={busy || !fileName.trim() || (!folder.trim())}
			onclick={submit}
		>
			{busy ? 'Starting…' : `Export .${extensionForFormat(format)}`}
		</Button>
	{/snippet}
</Modal>
