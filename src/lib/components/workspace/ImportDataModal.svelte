<script lang="ts">
	import { Button, Field, Input } from '$lib/components/ui'
	import { isTauri } from '$lib/api/tauri'
	import { modalPanel, overlayFade } from '$lib/utils/motion'
	import type { ImportFormat } from '$lib/api/engines'
	import {
		inferFormatFromPath,
		pickDataFile,
	} from '$lib/utils/exportTable'

	export type ImportFileSpec = {
		path: string
		format: ImportFormat
	}

	const FORMAT_LABELS: Record<string, string> = {
		csv: 'CSV',
		json: 'JSON',
		jsonl: 'JSON Lines',
		sql: 'SQL',
	}

	interface Props {
		open: boolean
		objectLabel?: string
		formats?: string[]
		busy?: boolean
		error?: string | null
		onconfirm: (spec: ImportFileSpec) => void
		oncancel: () => void
	}

	let {
		open,
		objectLabel = 'table',
		formats = ['csv', 'json', 'jsonl'],
		busy = false,
		error = null,
		onconfirm,
		oncancel,
	}: Props = $props()

	let path = $state('')
	let format = $state<ImportFormat | null>(null)
	let picking = $state(false)
	let wasOpen = $state(false)

	const desktop = isTauri()
	const fileLabel = $derived(path.split(/[/\\]/).pop() || path)
	const detectedLabel = $derived(
		format ? (FORMAT_LABELS[format] ?? format) : null,
	)
	const canContinue = $derived(!!path.trim() && !!format && desktop && !busy)

	$effect(() => {
		if (open && !wasOpen) {
			path = ''
			format = null
		}
		wasOpen = open
	})

	async function browseFile() {
		if (!desktop || busy || picking) return
		picking = true
		try {
			const picked = await pickDataFile(formats, path || null)
			if (!picked) return
			path = picked
			format = inferFormatFromPath(picked)
		} finally {
			picking = false
		}
	}

	function submit() {
		if (!canContinue || !format) return
		onconfirm({
			path: path.trim(),
			format,
		})
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
			transition:overlayFade
			onclick={() => {
				if (!busy) oncancel()
			}}
			disabled={busy}
		></button>
		<div
			class="relative z-10 w-full max-w-md rounded-lg border border-line bg-elevated p-4 shadow-lg"
			transition:modalPanel
			role="dialog"
			aria-modal="true"
			aria-labelledby="import-data-title"
		>
			<h2 id="import-data-title" class="m-0 text-body font-semibold text-fg">
				Import data
			</h2>
			<p class="mt-1.5 mb-3 text-label text-fg-muted">
				Choose a file to load into this {objectLabel}.
			</p>

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
					disabled={!canContinue}
					onclick={submit}
				>
					{busy ? 'Reading…' : 'Continue'}
				</Button>
			</div>
		</div>
	</div>
{/if}
