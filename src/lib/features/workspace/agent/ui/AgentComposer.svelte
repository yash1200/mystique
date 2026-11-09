<script lang="ts">
	import { Icon, IconButton, Select, Textarea } from '#core/ui'
	import { cn } from '#core/util'
	import { hasTableDrag, readTableDrag } from '#features/workspace/catalog'
	import type { Snippet } from 'svelte'
	import {
		activeMention,
		appendMention,
		insertMention,
		mentionCandidates,
		mentionLabel,
		type MentionTable,
	} from '../mentions'

	/**
	 * The message box. `@` opens table suggestions from the connection's
	 * catalog; a table dragged from the explorer drops in as `@schema.table`.
	 */
	interface Props {
		value: string
		sending: boolean
		canSend: boolean
		sendChord: string
		tables: MentionTable[]
		combo: {
			options: { value: string; label: string }[]
			value: string
			placeholder: string
			shortLabel: string
			disabled: boolean
		}
		oncombo: (value: string) => void
		onsend: () => void
		onstop: () => void
		footer?: Snippet
	}

	let {
		value = $bindable(''),
		sending,
		canSend,
		sendChord,
		tables,
		combo,
		oncombo,
		onsend,
		onstop,
		footer,
	}: Props = $props()

	const ID = 'agent-composer'
	let mention = $state<{ start: number; query: string } | null>(null)
	let highlighted = $state(0)
	let dropping = $state(false)
	const options = $derived(
		mention && tables.length > 0 ? mentionCandidates(tables, mention.query) : [],
	)
	const popupOpen = $derived(options.length > 0)

	function field(): HTMLTextAreaElement | null {
		return document.getElementById(ID) as HTMLTextAreaElement | null
	}

	function trackMention() {
		const el = field()
		if (!el) return
		const next = activeMention(value, el.selectionStart ?? value.length)
		if (next?.start !== mention?.start) highlighted = 0
		mention = next
	}

	function pick(table: MentionTable) {
		const el = field()
		if (!mention || !el) return
		const caret = el.selectionStart ?? value.length
		const next = insertMention(value, mention.start, caret, table)
		value = next.text
		mention = null
		queueMicrotask(() => {
			el.focus()
			el.setSelectionRange(next.caret, next.caret)
		})
	}

	function onkeydown(e: KeyboardEvent) {
		if (popupOpen) {
			if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
				e.preventDefault()
				const step = e.key === 'ArrowDown' ? 1 : -1
				highlighted = (highlighted + step + options.length) % options.length
				return
			}
			if ((e.key === 'Enter' && !e.metaKey && !e.ctrlKey) || e.key === 'Tab') {
				const table = options[highlighted]
				if (table) {
					e.preventDefault()
					pick(table)
				}
				return
			}
			if (e.key === 'Escape') {
				e.preventDefault()
				e.stopPropagation()
				mention = null
				return
			}
		}
		if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey) {
			e.preventDefault()
			e.stopPropagation()
			onsend()
		}
	}

	function ondragover(e: DragEvent) {
		if (!hasTableDrag(e.dataTransfer)) return
		e.preventDefault()
		if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy'
		dropping = true
	}

	function ondrop(e: DragEvent) {
		dropping = false
		const table = readTableDrag(e.dataTransfer)
		if (!table) return
		e.preventDefault()
		value = appendMention(value, table)
		queueMicrotask(() => {
			const el = field()
			el?.focus()
			el?.setSelectionRange(value.length, value.length)
		})
	}
</script>

<form
	class="relative shrink-0 bg-app p-2 pt-1"
	onsubmit={(e) => {
		e.preventDefault()
		onsend()
	}}
>
	{#if popupOpen}
		<ul
			class="absolute inset-x-2 bottom-full z-10 m-0 mb-1 max-h-56 list-none overflow-auto rounded-lg border border-line bg-elevated p-1 shadow-lg"
			role="listbox"
			aria-label="Tables"
		>
			{#each options as table, i (`${table.database}.${mentionLabel(table)}`)}
				<li role="option" aria-selected={i === highlighted}>
					<button
						type="button"
						class={cn(
							'flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-left font-mono text-label text-fg',
							i === highlighted ? 'bg-accent-muted' : 'hover:bg-hover',
						)}
						onmousedown={(e) => e.preventDefault()}
						onclick={() => pick(table)}
					>
						<Icon name="table" class="size-3.5 text-fg-muted" />
						<span class="min-w-0 truncate">{mentionLabel(table)}</span>
					</button>
				</li>
			{/each}
		</ul>
	{/if}
	<div
		class={cn(
			'overflow-hidden rounded-xl border border-line bg-elevated shadow-sm focus-within:border-focus focus-within:ring-2 focus-within:ring-accent-muted',
			dropping && 'border-focus ring-2 ring-accent-muted',
		)}
		role="group"
		aria-label="Message"
		{ondragover}
		ondragleave={() => (dropping = false)}
		{ondrop}
	>
		<Textarea
			id={ID}
			rows={3}
			resize="none"
			chrome={false}
			class="min-h-16 rounded-none px-3 pt-2.5 pb-1"
			placeholder="Ask for a query… (@ to reference a table)"
			bind:value
			disabled={sending}
			enterkeyhint="send"
			aria-autocomplete="list"
			aria-expanded={popupOpen}
			{onkeydown}
			oninput={trackMention}
			onclick={trackMention}
			onkeyup={(e) => {
				if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') trackMention()
			}}
			onblur={() => (mention = null)}
		/>
		<div class="flex items-center justify-between gap-2 px-2 pt-0.5 pb-1.5">
			<Select
				variant="quiet"
				inputSize="sm"
				options={combo.options}
				value={combo.value}
				placeholder={combo.placeholder}
				triggerLabel={combo.shortLabel}
				ariaLabel="Model"
				disabled={combo.disabled}
				onchange={oncombo}
			>
				{#snippet leading()}
					<Icon name="sparkle" class="size-3.5" />
				{/snippet}
			</Select>
			<div class="flex min-w-0 items-center gap-2">
				{@render footer?.()}
				{#if sending}
					<IconButton type="button" label="Stop" variant="default" size="sm" onclick={onstop}>
						<Icon name="close" />
					</IconButton>
				{:else}
					<IconButton
						type="submit"
						label={`Send (${sendChord})`}
						variant="accent"
						size="sm"
						disabled={!canSend}
					>
						<Icon name="send" />
					</IconButton>
				{/if}
			</div>
		</div>
	</div>
</form>
