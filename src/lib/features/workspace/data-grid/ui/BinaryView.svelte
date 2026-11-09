<script lang="ts">
	/** Hex dump with an ASCII column, copy as hex / base64, and an image preview. */
	import { Button } from '#core/ui'
	import { copyTextToClipboard } from '#core/util'
	import {
		binaryBytes,
		formatBytes,
		hexDump,
		imageMime,
		toBase64,
		toHex,
	} from '../binary-dump'
	import { PARTIAL_COPY_MESSAGE } from '../large-cell'

	interface Props {
		/** `\x…` hex or a binary column's text. */
		text: string
		/** Only part of the value is here (a large cell still loading or unkeyed). */
		partial?: boolean
		/** Full size when `text` is a preview. */
		totalBytes?: number | null
	}

	let { text, partial = false, totalBytes = null }: Props = $props()

	/** Rows rendered: 64 KB. The copy buttons still take every byte. */
	const MAX_ROWS = 4096

	const bytes = $derived(binaryBytes(text))
	const rows = $derived(hexDump(bytes, MAX_ROWS))
	const mime = $derived(imageMime(bytes))
	const shownAll = $derived(bytes.length <= MAX_ROWS * 16)
	let note = $state('')
	let imageUrl = $state<string | null>(null)
	let imageSize = $state<{ w: number; h: number } | null>(null)

	$effect(() => {
		// A cut image would not decode; wait for every byte.
		if (!mime || partial) {
			imageUrl = null
			return
		}
		const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime }))
		imageUrl = url
		imageSize = null
		return () => URL.revokeObjectURL(url)
	})

	async function copy(kind: 'hex' | 'base64') {
		const payload = kind === 'hex' ? toHex(bytes) : toBase64(bytes)
		const res = await copyTextToClipboard(payload)
		note = !res.ok
			? 'Copy failed'
			: partial
				? PARTIAL_COPY_MESSAGE
				: kind === 'hex'
					? 'Copied as hex.'
					: 'Copied as base64.'
	}
</script>

<div class="flex min-h-0 flex-1 flex-col gap-2">
	<div class="flex flex-wrap items-center justify-between gap-2">
		<p class="m-0 text-caption text-fg-muted tabular-nums">
			{formatBytes(totalBytes ?? bytes.length)}{#if mime}
				<span aria-hidden="true"> · </span>{mime.slice(6).toUpperCase()}{#if imageSize}
					<span aria-hidden="true"> · </span>{imageSize.w}×{imageSize.h}{/if}{/if}{#if partial}
				<span aria-hidden="true"> · </span>preview{/if}{#if !shownAll}
				<span aria-hidden="true"> · </span>first {formatBytes(MAX_ROWS * 16)} shown{/if}
			{#if note}<span aria-hidden="true"> · </span>{note}{/if}
		</p>
		<div class="flex items-center gap-1">
			<Button variant="ghost" size="sm" onclick={() => void copy('hex')}>Copy as hex</Button>
			<Button variant="ghost" size="sm" onclick={() => void copy('base64')}>Copy as base64</Button>
		</div>
	</div>
	{#if imageUrl}
		<div class="flex max-h-48 shrink-0 justify-center overflow-hidden rounded-md border border-line bg-app p-2">
			<img
				src={imageUrl}
				alt="Preview of the stored bytes"
				class="max-h-44 max-w-full object-contain"
				onload={(e) => {
					const img = e.currentTarget as HTMLImageElement
					imageSize = { w: img.naturalWidth, h: img.naturalHeight }
				}}
			/>
		</div>
	{/if}
	<div
		class="min-h-40 flex-1 overflow-auto rounded-md border border-line bg-app px-2 py-1.5 font-mono text-label leading-relaxed"
		role="region"
		aria-label="Hex dump"
	>
		{#if rows.length === 0}
			<p class="m-0 text-fg-muted">Empty value (0 bytes)</p>
		{:else}
			{#each rows as row (row.offset)}
				<div class="flex gap-4 whitespace-pre">
					<span class="text-fg-muted select-none">{row.offset}</span>
					<span class="text-fg">{row.hex}</span>
					<span class="text-fg-secondary">{row.ascii}</span>
				</div>
			{/each}
		{/if}
	</div>
</div>
