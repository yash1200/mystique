<script lang="ts">
	import { Button, Field, Input, Select } from '$lib/components/ui'
	import type { SelectOption } from '$lib/components/ui'
	import { isTauri } from '$lib/api/tauri'
	import {
		applyFormatExtension,
		defaultDownloadsDir,
		extensionForFormat,
		pickDirectory,
		safeExportFileBase,
		type TableFileFormat,
	} from '$lib/utils/exportTable'

	export type ExportDataSpec = {
		fileName: string
		folder: string
		format: TableFileFormat
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
		onconfirm,
		oncancel,
	}: Props = $props()

	let fileName = $state('')
	let folder = $state('')
	let format = $state<TableFileFormat>('csv')
	let folderBusy = $state(false)

	const formatOptions = $derived.by((): SelectOption[] => {
		const ids = formats.length > 0 ? formats : ['csv']
		return ids.map((id) => ({
			value: id,
			label: FORMAT_LABELS[id] ?? id.toUpperCase(),
		}))
	})

	const previewName = $derived(applyFormatExtension(fileName, format))
	const desktop = isTauri()

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
		void loadDownloads()
	})

	async function loadDownloads() {
		const dir = await defaultDownloadsDir()
		if (dir) folder = dir
	}

	async function browseFolder() {
		if (!desktop || busy || folderBusy) return
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
		if (desktop && !folder.trim()) return
		onconfirm({
			fileName: applyFormatExtension(name, format),
			folder: folder.trim(),
			format,
		})
	}

	function onFormatChange(value: string) {
		format = value as TableFileFormat
	}

	function onWindowKey(e: KeyboardEvent) {
		if (open && e.key === 'Escape' && !busy) oncancel()
	}
</script>

<svelte:window onkeydown={onWindowKey} />

{#if open}
	<div class="fixed inset-0 z-50 flex items-center justify-center p-4">
		<button
			type="button"
			class="absolute inset-0 border-none bg-black/45"
			aria-label="Dismiss"
			onclick={() => {
				if (!busy) oncancel()
			}}
			disabled={busy}
		></button>
		<div
			class="relative z-10 w-full max-w-md rounded-lg border border-line bg-elevated p-4 shadow-lg"
			role="dialog"
			aria-modal="true"
			aria-labelledby="export-data-title"
		>
			<h2 id="export-data-title" class="m-0 text-body font-semibold text-fg">
				Export data
			</h2>
			<p class="mt-1.5 mb-3 text-label text-fg-muted">
				Write the full {objectLabel} to a file.
			</p>

			<div class="flex flex-col gap-3">
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
								disabled={busy || !desktop}
								placeholder={desktop
									? 'Choose a folder…'
									: 'Downloads (browser download)'}
							/>
						</div>
						<Button
							variant="secondary"
							size="sm"
							disabled={busy || folderBusy || !desktop}
							onclick={() => void browseFolder()}
						>
							Browse
						</Button>
					</div>
					{#if !desktop}
						<p class="mt-1 mb-0 text-caption text-fg-muted">
							Folder picker requires the desktop app.
						</p>
					{/if}
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
			</div>

			{#if error}
				<p class="mt-3 mb-0 text-caption text-danger" role="alert">{error}</p>
			{/if}

			<div class="mt-4 flex flex-wrap items-center justify-end gap-2">
				<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
					Cancel
				</Button>
				<Button
					variant="primary"
					size="sm"
					disabled={busy ||
						!fileName.trim() ||
						(desktop && !folder.trim())}
					onclick={submit}
				>
					{busy ? 'Exporting…' : `Export .${extensionForFormat(format)}`}
				</Button>
			</div>
		</div>
	</div>
{/if}
