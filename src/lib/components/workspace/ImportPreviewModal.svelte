<script lang="ts">
	import { Button, Field, Select } from '$lib/components/ui'
	import type { SelectOption } from '$lib/components/ui'
	import type {
		ImportColumnMap,
		ImportEncoding,
		ImportOnDuplicate,
		PreviewImportResult,
	} from '$lib/api/engines'
	import { cn } from '$lib/utils/cn'

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

	export type ImportRunSpec = {
		mapping: ImportColumnMap[]
		encoding: ImportEncoding
		onDuplicate: ImportOnDuplicate
	}

	interface Props {
		open: boolean
		objectName: string
		preview: PreviewImportResult | null
		busy?: boolean
		error?: string | null
		onencoding: (encoding: ImportEncoding) => void
		onconfirm: (spec: ImportRunSpec) => void
		onback: () => void
		oncancel: () => void
	}

	let {
		open,
		objectName,
		preview,
		busy = false,
		error = null,
		onencoding,
		onconfirm,
		onback,
		oncancel,
	}: Props = $props()

	let mapping = $state<ImportColumnMap[]>([])
	let encoding = $state<ImportEncoding>('utf8')
	let onDuplicate = $state<ImportOnDuplicate>('stopOnError')
	let seededKeys = $state('')

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
		if (busy || !preview) return
		if (!mappedCount) return
		onconfirm({
			mapping: mapping.map((m) => ({
				fileKey: m.fileKey,
				tableColumn: (m.tableColumn ?? '').trim() || null,
			})),
			encoding,
			onDuplicate,
		})
	}

	function onWindowKey(e: KeyboardEvent) {
		if (open && e.key === 'Escape' && !busy) oncancel()
	}
</script>

<svelte:window onkeydown={onWindowKey} />

{#if open && preview}
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
			class="relative z-10 flex max-h-[min(44rem,92vh)] w-full max-w-panel-lg flex-col overflow-hidden rounded-lg border border-line bg-elevated shadow-lg"
			role="dialog"
			aria-modal="true"
			aria-labelledby="import-preview-title"
		>
			<header class="shrink-0 border-b border-line px-4 py-3">
				<h2
					id="import-preview-title"
					class="m-0 text-body font-semibold text-fg"
				>
					Review import · {objectName}
				</h2>
				<p class="mt-1 mb-0 text-label text-fg-muted">
					First {preview.rows.length} row{preview.rows.length === 1
						? ''
						: 's'} from the file. Use the dropdowns to map each file
					field to a database column.
				</p>
			</header>

			<div class="min-h-0 flex-1 overflow-auto px-4 py-3">
				<div class="grid gap-3 sm:grid-cols-2">
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
					Preview
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
												!(map.tableColumn ?? '').trim() &&
													'text-fg-muted',
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

				{#if error}
					<p class="mt-3 mb-0 text-caption text-danger" role="alert">{error}</p>
				{/if}
			</div>

			<footer
				class="flex shrink-0 flex-wrap items-center justify-end gap-2 border-t border-line px-4 py-3"
			>
				<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
					Cancel
				</Button>
				<Button variant="secondary" size="sm" disabled={busy} onclick={onback}>
					Back
				</Button>
				<Button
					variant="primary"
					size="sm"
					disabled={busy || mappedCount === 0}
					onclick={submit}
				>
					{busy ? 'Importing…' : 'Start import'}
				</Button>
			</footer>
		</div>
	</div>
{/if}
