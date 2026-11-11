<script lang="ts">
	import { Button, Checkbox, Field, Icon, Input, Modal, Segmented } from '#core/ui'
	import type { TableFileFormat } from '#core/util'
	import {
		applyFormatExtension,
		defaultDownloadsDir,
		extensionForFormat,
		pickDirectory,
		safeExportFileBase,
	} from '#core/util'
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

	const FORMAT_NOTES: Record<string, string> = {
		csv: 'Comma-separated, header row first.',
		json: 'An array of row objects.',
		jsonl: 'One JSON object per line.',
		sql: 'INSERT statements you can replay elsewhere.',
	}

	interface Props {
		open: boolean
		objectName: string
		objectLabel?: string
		/** The connection it reads from, for the footer. */
		connectionName?: string
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
		connectionName,
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

	const formatOptions = $derived.by(() => {
		const ids = formats.length > 0 ? formats : ['csv']
		return ids.map((id) => ({
			value: id,
			label: FORMAT_LABELS[id] ?? id.toUpperCase(),
		}))
	})

	const previewName = $derived(applyFormatExtension(fileName, format))
	const ext = $derived(extensionForFormat(format))
	/** The name without the extension the format adds (shown beside the field). */
	const baseName = $derived(fileName.replace(new RegExp(`\\.${ext}$`, 'i'), ''))

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
</script>

<Modal
	{open}
	onclose={oncancel}
	icon="download"
	description="Write the full {objectLabel} to a file. It runs in the background."
	note={connectionName ? `Reads from ${connectionName} · no writes` : 'Reads only · no writes'}
	size="md"
	dismissible={!busy}
>
	{#snippet titleContent()}
		Export <span class="font-mono font-medium">{objectName}</span>
	{/snippet}

	<Field label="Format" hint={FORMAT_NOTES[format]}>
		<Segmented
			label="Format"
			class="self-start"
			value={format}
			options={formatOptions}
			disabled={busy}
			onchange={(v) => (format = v as TableFileFormat)}
		/>
	</Field>

	<Field label="File name" htmlFor="export-data-name">
		<div class="relative">
			<Input
				id="export-data-name"
				value={baseName}
				oninput={(e) => (fileName = e.currentTarget.value)}
				disabled={busy}
				placeholder="export"
				class="pr-16"
				autofocus
			/>
			<span class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 font-mono text-data text-fg-muted">
				.{ext}
			</span>
		</div>
		<span class="text-label text-fg-muted">
			Saves as <span class="font-mono text-fg-secondary">{previewName}</span>
		</span>
	</Field>

	<Field label="Folder" htmlFor="export-data-folder">
		<div class="flex min-w-0 gap-2">
			<Input
				id="export-data-folder"
				class="min-w-0 flex-1 font-mono text-data"
				bind:value={folder}
				disabled={busy}
				placeholder="Choose a folder…"
			/>
			<Button class="shrink-0" disabled={busy || folderBusy} onclick={() => void browseFolder()}>
				Browse…
			</Button>
		</div>
	</Field>

	{#if txnTab}
		<div class="flex flex-col gap-1">
			<Checkbox
				bind:checked={throughTransaction}
				disabled={busy}
				label="Include uncommitted changes from “{txnTab.title}”"
			/>
			<p class="m-0 text-label text-fg-muted">
				{throughTransaction
					? 'Reads through that tab’s open transaction, as the tab sees the data. The tab waits while the export runs.'
					: 'Reads committed data only.'}
			</p>
		</div>
	{/if}

	{#if error}
		<p class="m-0 text-label text-danger" role="alert">{error}</p>
	{/if}

	{#snippet footer()}
		<Button disabled={busy} onclick={oncancel}>Cancel</Button>
		<Button variant="primary" disabled={busy || !fileName.trim() || !folder.trim()} onclick={submit}>
			{#if busy}
				Starting…
			{:else}
				<Icon name="download" />Export .{ext}
			{/if}
		</Button>
	{/snippet}
</Modal>
