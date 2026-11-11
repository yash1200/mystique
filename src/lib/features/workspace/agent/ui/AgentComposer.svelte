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
		/** Before the model picker: the agent picker. */
		agentPicker?: Snippet
		/** An external agent picks its own model: shown instead of the picker. */
		fixedModel?: string | null
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
		agentPicker,
		footer,
		fixedModel = null,
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
	class="relative shrink-0 px-3 pt-1 pb-3"
	onsubmit={(e) => {
		e.preventDefault()
		onsend()
	}}
>
	{#if popupOpen}
		<ul
			class="absolute inset-x-3 bottom-full z-10 m-0 mb-1 max-h-56 list-none overflow-auto rounded-[11px] border border-line bg-raised p-[5px] shadow-md"
			role="listbox"
			aria-label="Tables"
		>
			{#each options as table, i (`${table.database}.${mentionLabel(table)}`)}
				<li role="option" aria-selected={i === highlighted}>
					<button
						type="button"
						class={cn(
							'flex h-[1.875rem] w-full items-center gap-2 rounded-[7px] px-[9px] text-left font-mono text-label text-fg',
							i === highlighted ? 'bg-hover' : 'hover:bg-hover',
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
			'overflow-hidden rounded-xl border border-line-strong bg-field transition-[border-color,box-shadow] duration-150',
			'focus-within:border-accent focus-within:shadow-[0_0_0_3px_var(--accent-soft)]',
			dropping && 'border-accent shadow-[0_0_0_3px_var(--accent-soft)]',
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
			placeholder="Ask for a query · @ to mention a table"
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
		<div class="flex items-center gap-1 px-1.5 pt-0.5 pb-1.5">
			{@render agentPicker?.()}
			{#if fixedModel}
				<span
					class="flex min-w-0 items-center gap-1.5 px-1.5 text-caption text-fg-secondary"
					title="External agent (ACP). It picks its own model."
				>
					<Icon name="sparkle" class="size-3.5 shrink-0" />
					<span class="truncate">{fixedModel}</span>
				</span>
			{:else}
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
			{/if}
			<div class="ml-auto flex min-w-0 items-center gap-2">
				{@render footer?.()}
				{#if sending}
					<IconButton type="button" label="Stop" variant="default" size="sm" onclick={onstop}>
						<Icon name="stop" />
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
