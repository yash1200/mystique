<script lang="ts">
	import { Alert, EmptyHint, Icon, IconButton, Pill, SideDrawer } from '#core/ui'
	import { copyTextToClipboard, highlightCode, tokenClass } from '#core/util'

	interface Props {
		statements: string[]
		error?: string | null
		onclose: () => void
	}

	let { statements, error = null, onclose }: Props = $props()

	type Kind = 'insert' | 'update' | 'delete' | 'other'

	const TONES = {
		insert: 'success',
		update: 'warning',
		delete: 'danger',
		other: 'neutral',
	} as const

	/** The verb a statement starts with, past comments and whitespace. */
	function kindOf(sql: string): Kind {
		const head = sql.replace(/^(\s|--[^\n]*\n|\/\*[\s\S]*?\*\/)*/, '').slice(0, 8).toLowerCase()
		if (head.startsWith('insert')) return 'insert'
		if (head.startsWith('update')) return 'update'
		if (head.startsWith('delete')) return 'delete'
		return 'other'
	}

	const items = $derived(
		statements.map((sql) => {
			const text = `${sql.trim().replace(/;+$/, '')};`
			return { text, kind: kindOf(sql), tokens: highlightCode('sql', text) }
		}),
	)

	/** "2 updates · 1 insert" */
	const summary = $derived.by(() => {
		const counts = new Map<Kind, number>()
		for (const item of items) counts.set(item.kind, (counts.get(item.kind) ?? 0) + 1)
		const word: Record<Kind, string> = { insert: 'insert', update: 'update', delete: 'delete', other: 'statement' }
		return (['update', 'insert', 'delete', 'other'] as Kind[])
			.filter((k) => counts.has(k))
			.map((k) => `${counts.get(k)} ${word[k]}${counts.get(k) === 1 ? '' : 's'}`)
			.join(' · ')
	})

	/** Index of the statement just copied, or -1 for all of them. */
	let copied = $state<number | null>(null)
	let copiedTimer: ReturnType<typeof setTimeout> | undefined

	async function copy(which: number) {
		const text = which < 0 ? items.map((i) => i.text).join('\n\n') : (items[which]?.text ?? '')
		if (!(await copyTextToClipboard(text)).ok) return
		copied = which
		clearTimeout(copiedTimer)
		copiedTimer = setTimeout(() => (copied = null), 1500)
	}

	$effect(() => () => clearTimeout(copiedTimer))
</script>

<SideDrawer title="Pending SQL" icon="code" closeLabel="Hide pending SQL" {onclose}>
	{#snippet badge()}
		{#if items.length}<Pill>{items.length}</Pill>{/if}
	{/snippet}
	{#snippet actions()}
		<IconButton
			label={copied === -1 ? 'Copied' : 'Copy all'}
			size="sm"
			variant="quiet"
			disabled={!items.length}
			onclick={() => void copy(-1)}
		>
			<Icon name={copied === -1 ? 'check' : 'copy'} />
		</IconButton>
	{/snippet}

	{#if error}
		<div class="p-3">
			<Alert tone="danger">{error}</Alert>
		</div>
	{:else if items.length === 0}
		<EmptyHint
			fill
			icon="code"
			title="No pending changes"
			description="Edit a cell, add or delete a row, and the SQL that Save will run shows here."
		/>
	{:else}
		<ol class="m-0 flex min-h-0 flex-1 list-none flex-col gap-2 overflow-auto bg-app p-3">
			{#each items as item, i (i)}
				<li class="group overflow-hidden rounded-lg bg-field shadow-[inset_0_0_0_1px_var(--line)]">
					<div class="flex h-8 items-center gap-2 border-b border-line pr-1 pl-3">
						<Pill tone={TONES[item.kind]} uppercase compact>{item.kind === 'other' ? 'SQL' : item.kind}</Pill>
						<span class="font-mono text-caption text-fg-muted tabular-nums">#{i + 1}</span>
						<span class="flex-1"></span>
						<span class="opacity-0 transition-opacity duration-150 group-hover:opacity-100 focus-within:opacity-100">
							<IconButton
								label={copied === i ? 'Copied' : 'Copy statement'}
								size="sm"
								variant="quiet"
								onclick={() => void copy(i)}
							>
								<Icon name={copied === i ? 'check' : 'copy'} />
							</IconButton>
						</span>
					</div>
					<pre
						class="m-0 px-3 py-2.5 font-mono text-label leading-relaxed break-words whitespace-pre-wrap text-fg select-text">{#each item.tokens as token, t (t)}<span
								class={tokenClass(token.kind)}>{token.text}</span
							>{/each}</pre>
				</li>
			{/each}
		</ol>
		<footer class="flex h-9 shrink-0 items-center border-t border-line px-3 text-caption text-fg-muted">
			{summary}
		</footer>
	{/if}
</SideDrawer>
