<script lang="ts">
	import type { ParsedConnectionUrl } from '#core/contracts'
	import { isTauri, parseEngineConnectionUrl } from '#core/ipc'
	import { runtimeFor } from '../products'
	import { Alert, Button, Icon, IconButton } from '#core/ui'
	import { DURATION, cn, easeOut, reducedMotion } from '#core/util'
	import { onMount } from 'svelte'
	import { fade } from 'svelte/transition'

	interface Props {
		/** Product or runtime id — backend maps products to the URL parser. */
		engine: string
		/** Placeholder shown in the URL field. */
		placeholder?: string
		disabled?: boolean
		/** Apply parsed fields into the surrounding form. */
		onparsed?: (parsed: ParsedConnectionUrl) => void
	}

	let {
		engine,
		placeholder = 'postgres://user:pass@host:5432/dbname',
		disabled = false,
		onparsed,
	}: Props = $props()

	let url = $state('')
	let open = $state(false)
	let importing = $state(false)
	let pasting = $state(false)
	let error = $state<string | null>(null)
	let success = $state<string | null>(null)
	let urlEl = $state<HTMLTextAreaElement | null>(null)

	const hasUrl = $derived(url.trim().length > 0)
	const busy = $derived(disabled || importing)

	const runtime = $derived(runtimeFor(engine) ?? engine)
	const schemeHint = $derived(
		runtime === 'postgres'
			? 'postgres:// or postgresql://'
			: 'mysql:// or mariadb://',
	)

	const fillChips = [
		'Host',
		'Port',
		'Database',
		'User',
		'Password',
		'SSL',
	] as const

	function clearFeedback() {
		error = null
		success = null
	}

	function openPanel() {
		if (busy) return
		open = true
		requestAnimationFrame(() => urlEl?.focus())
	}

	function toggleOpen() {
		if (busy) return
		if (open) {
			open = false
			clearFeedback()
			return
		}
		openPanel()
	}

	onMount(() => {
		const onMenu = () => openPanel()
		window.addEventListener('mystique:import-connection-url', onMenu)
		return () =>
			window.removeEventListener('mystique:import-connection-url', onMenu)
	})

	async function pasteFromClipboard() {
		if (busy || !navigator.clipboard?.readText) {
			error = 'Clipboard paste is not available here.'
			return
		}
		pasting = true
		clearFeedback()
		try {
			const text = (await navigator.clipboard.readText()).trim()
			if (!text) {
				error = 'Clipboard is empty.'
				return
			}
			url = text
			if (!open) open = true
			requestAnimationFrame(() => urlEl?.focus())
		} catch {
			error = 'Could not read clipboard. Paste with ⌘V instead.'
		} finally {
			pasting = false
		}
	}

	async function applyUrl() {
		clearFeedback()
		const trimmed = url.trim()
		if (!trimmed) {
			error = 'Paste a connection URL first.'
			return
		}
		if (!isTauri()) {
			error = 'URL import needs the Mystique desktop app.'
			return
		}
		importing = true
		try {
			const parsed = await parseEngineConnectionUrl({ engine, url: trimmed })
			onparsed?.(parsed)
			success = 'Server fields updated from URL.'
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		} finally {
			importing = false
		}
	}

	function onUrlKeydown(e: KeyboardEvent) {
		if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
			e.preventDefault()
			if (hasUrl && !busy) void applyUrl()
		}
	}

	/** Expand body: opacity + slight lift, respects reduced motion. */
	function expandPanel(_node: Element) {
		const reduce = reducedMotion()
		return {
			duration: reduce ? 120 : DURATION.popover,
			easing: easeOut,
			css: (t: number) => {
				const y = reduce ? 0 : (1 - t) * 6
				return `opacity:${t};transform:translateY(${y}px)`
			},
		}
	}
</script>

<div
	class={cn(
		'group/import relative mb-4 overflow-hidden rounded-lg border transition-[border-color,box-shadow,background-color] duration-200',
		'ease-out',
		open
			? 'border-accent/35 bg-elevated shadow-sm'
			: 'border-line bg-input/60 hover:border-line-strong hover:bg-input',
	)}
>
	<!-- Active accent rail -->
	<div
		class={cn(
			'pointer-events-none absolute inset-y-0 left-0 w-0.5 bg-accent transition-opacity duration-200',
			open ? 'opacity-100' : 'opacity-0',
		)}
		aria-hidden="true"
	></div>

	<button
		type="button"
		class={cn(
			'relative flex w-full items-center gap-3 px-3.5 py-3 text-left',
			'focus:outline-none focus-visible:ring-2 focus-visible:ring-accent-muted',
			busy && 'opacity-70',
		)}
		aria-expanded={open}
		aria-controls="import-url-panel"
		id="import-url-trigger"
		onclick={toggleOpen}
		disabled={busy}
	>
		<span
			class={cn(
				'grid size-8 shrink-0 place-items-center rounded-md border transition-colors duration-150',
				open
					? 'border-accent/30 bg-accent-muted text-accent'
					: 'border-line bg-elevated text-fg-secondary group-hover/import:border-line-strong group-hover/import:text-fg',
			)}
			aria-hidden="true"
		>
			<Icon name="external" class="size-4" />
		</span>

		<span class="min-w-0 flex-1">
			<span class="flex flex-wrap items-center gap-x-2 gap-y-0.5">
				<span class="text-body font-semibold tracking-tight text-fg">
					Import from URL
				</span>
				<span
					class="rounded-sm border border-line bg-elevated px-1.5 py-px font-mono text-micro text-fg-muted"
				>
					{schemeHint}
				</span>
			</span>
			<span class="mt-0.5 block text-caption text-fg-muted">
				Paste a connection string to fill the fields below
			</span>
		</span>

		<span
			class={cn(
				'grid size-7 shrink-0 place-items-center rounded-md text-fg-muted transition-transform duration-200',
				'ease-out',
				open && 'rotate-180 bg-hover text-fg',
			)}
			aria-hidden="true"
		>
			<Icon name="chevron-down" class="size-3.5" />
		</span>
	</button>

	{#if open}
		<div
			id="import-url-panel"
			role="region"
			aria-labelledby="import-url-trigger"
			class="relative border-t border-line px-3.5 pt-3 pb-3.5"
			transition:expandPanel
		>
			<div class="space-y-3">
				<div class="space-y-1.5">
					<div class="flex items-center justify-between gap-2">
						<label
							for="import-conn-url"
							class="text-label font-medium text-fg-secondary"
						>
							Connection URL
						</label>
						<div class="flex items-center gap-1">
							<IconButton
								label="Paste from clipboard"
								variant="quiet"
								size="sm"
								onclick={pasteFromClipboard}
								disabled={busy || pasting}
							>
								<Icon name="copy" class="size-3.5" />
							</IconButton>
							{#if hasUrl}
								<button
									type="button"
									class="rounded-sm px-1.5 py-0.5 text-caption text-fg-muted transition-colors hover:bg-hover hover:text-fg"
									onclick={() => {
										url = ''
										clearFeedback()
										urlEl?.focus()
									}}
									disabled={busy}
								>
									Clear
								</button>
							{/if}
						</div>
					</div>

					<div
						class={cn(
							'relative rounded-md border bg-app transition-[border-color,box-shadow] duration-150',
							'focus-within:border-focus focus-within:ring-2 focus-within:ring-accent-muted',
							error
								? 'border-danger/40'
								: success
									? 'border-success/40'
									: 'border-line',
						)}
					>
						<textarea
							bind:this={urlEl}
							id="import-conn-url"
							rows={2}
							class={cn(
								'min-h-14 w-full resize-y rounded-md border-0 bg-transparent px-3 py-2.5',
								'font-mono text-label leading-relaxed text-fg',
								'placeholder:text-fg-muted/80 focus:outline-none',
								'disabled:opacity-55',
							)}
							bind:value={url}
							{placeholder}
							autocomplete="off"
							spellcheck={false}
							disabled={busy}
							onkeydown={onUrlKeydown}
						></textarea>
					</div>

					<div class="flex flex-wrap items-center gap-1.5">
						<span class="text-caption text-fg-muted">Fills</span>
						{#each fillChips as chip (chip)}
							<span
								class="inline-flex items-center rounded-sm border border-line bg-elevated px-1.5 py-px font-mono text-micro text-fg-secondary"
							>
								{chip}
							</span>
						{/each}
						<span class="text-caption text-fg-muted">· ⌘↵ apply</span>
					</div>
				</div>

				<div class="flex flex-wrap items-center justify-end gap-2">
					{#if success}
						<span
							class="mr-auto inline-flex items-center gap-1.5 text-caption font-medium text-success"
							transition:fade={{ duration: 140 }}
							role="status"
						>
							<span
								class="grid size-4 place-items-center rounded-full bg-success/15"
								aria-hidden="true"
							>
								<Icon name="check" class="size-2.5 text-success" />
							</span>
							{success}
						</span>
					{/if}
					<Button
						type="button"
						variant="primary"
						size="sm"
						onclick={applyUrl}
						disabled={busy || !hasUrl}
					>
						{#if importing}
							<Icon name="refresh" class="size-3.5 animate-spin" />
							Importing…
						{:else}
							Apply URL
						{/if}
					</Button>
				</div>

				{#if error}
					<div transition:fade={{ duration: 140 }}>
						<Alert tone="danger">{error}</Alert>
					</div>
				{/if}
			</div>
		</div>
	{/if}
</div>
