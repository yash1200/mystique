<script lang="ts">
	/**
	 * The query tab's one toolbar, under the editor where it meets the results. Left: what you do to the
	 * query (run, explain, format). Right: the state around it (transaction,
	 * save) and the agent. Rare choices live in menus, not buttons.
	 */
	import {
		Button,
		ContextMenu,
		Icon,
		IconButton,
		Input,
		Pill,
		Spinner,
		Switch,
		type ContextMenuItem,
	} from '#core/ui'
	import { isMacPlatform, keyCap } from '#features/shortcuts'
	import { appPrefs } from '#lib/app'

	interface Props {
		running: boolean
		canRun: boolean
		/** The engine can cancel a running query. */
		canCancel: boolean
		onrun: () => void
		oncancel: () => void
		onformat: () => void
		/** The engine can explain a plan. */
		explain?: boolean
		onexplain?: (analyze: boolean) => void
		/** Mongo: the collection the shell runs against. */
		mql?: boolean
		collection?: string
		/** The engine can hold a transaction on one connection. */
		transactions?: boolean
		manualCommit?: boolean
		transactionOpen?: boolean
		onmanualcommit?: (manual: boolean) => void
		oncommit?: () => void
		onrollback?: () => void
		onsave?: () => void
		dirty?: boolean
		saving?: boolean
		/** Completion, folding and the run gutter are off for this document. */
		largeDoc?: boolean
	}

	let {
		running,
		canRun,
		canCancel,
		onrun,
		oncancel,
		onformat,
		explain = false,
		onexplain,
		mql = false,
		collection = $bindable(''),
		transactions = false,
		manualCommit = false,
		transactionOpen = false,
		onmanualcommit,
		oncommit,
		onrollback,
		onsave,
		dirty = false,
		saving = false,
		largeDoc = false,
	}: Props = $props()

	const mac = isMacPlatform()
	const mod = keyCap('Mod', 'glyphs', mac)
	const chord = (key: string, shift = false) =>
		mac ? `${mod}${shift ? '⇧' : ''}${key}` : `Ctrl+${shift ? 'Shift+' : ''}${key}`

	const EXPLAIN_ITEMS: ContextMenuItem[] = [
		{ id: 'plan', label: 'Explain' },
		{ id: 'analyze', label: 'Explain with actual rows' },
	]

	let explainMenu = $state<{ x: number; y: number } | null>(null)

	function openExplainMenu(e: MouseEvent) {
		if (explainMenu) {
			explainMenu = null
			return
		}
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
		explainMenu = { x: rect.left, y: rect.bottom + 4 }
	}

	function pickExplain(id: string) {
		explainMenu = null
		onexplain?.(id === 'analyze')
	}
</script>

<div
	class="flex h-11 shrink-0 items-center gap-1 border-t border-line bg-surface px-2"
	role="toolbar"
	aria-label="Query"
>
	{#if running}
		<Button
			variant="secondary"
			size="sm"
			disabled={!canCancel}
			title={canCancel ? 'Cancel (Esc)' : 'This engine cannot cancel a running query'}
			onclick={oncancel}
		>
			{#if canCancel}
				<Icon name="stop" />Stop
			{:else}
				<Spinner size="sm" />Running…
			{/if}
		</Button>
	{:else}
		<Button
			variant="primary"
			size="sm"
			disabled={!canRun}
			title="Run the statement at the cursor ({chord('↵')}) · all ({chord('↵', true)})"
			onclick={onrun}
		>
			<Icon name="play" />Run
			<kbd class="ml-0.5 h-4 min-w-0 border-0 bg-black/10 px-1 text-[0.625rem] text-current">
				{chord('↵')}
			</kbd>
		</Button>
	{/if}

	{#if explain}
		<div class="ml-1 flex items-center">
			<Button
				variant="ghost"
				size="sm"
				class="pr-1.5"
				disabled={!canRun || running}
				title="Explain the statement at the cursor"
				onclick={() => onexplain?.(false)}
			>
				Explain
			</Button>
			<IconButton
				label="More explain options"
				variant="quiet"
				size="sm"
				class="w-5"
				disabled={!canRun || running}
				active={explainMenu != null}
				aria-haspopup="menu"
				aria-expanded={explainMenu != null}
				onclick={openExplainMenu}
			>
				<Icon name="chevron-down" />
			</IconButton>
		</div>
	{/if}
	<IconButton
		label="Format ({chord('F', true)})"
		variant="quiet"
		size="sm"
		disabled={!canRun || running}
		onclick={onformat}
	>
		<Icon name="format" />
	</IconButton>

	{#if mql}
		<span class="mx-1.5 h-4 w-px bg-line" aria-hidden="true"></span>
		<Input
			class="w-36 font-mono"
			inputSize="sm"
			placeholder="collection"
			aria-label="Collection"
			bind:value={collection}
		/>
	{/if}

	<div class="ml-auto flex min-w-0 items-center gap-1">
		{#if largeDoc}
			<span title="Completion, folding and the run gutter are off for a document this large. Run and statement keys still work.">
				<Pill tone="info">Large file</Pill>
			</span>
		{/if}

		{#if transactions}
			{#if manualCommit && transactionOpen}
				<Pill tone="warning" uppercase>Transaction open</Pill>
				<Button size="sm" disabled={running} onclick={() => oncommit?.()}>Commit</Button>
				<Button variant="ghost" size="sm" disabled={running} onclick={() => onrollback?.()}>
					Rollback
				</Button>
				<span class="mx-1 h-4 w-px bg-line" aria-hidden="true"></span>
			{/if}
			<span
				class="flex items-center gap-2 px-1.5 text-label text-fg-secondary"
				title={manualCommit
					? 'Off: the first write opens a transaction; Commit or Rollback ends it.'
					: 'Each statement commits on its own. Turning it off pins this tab to one connection.'}
			>
				Auto-commit
				<Switch
					checked={!manualCommit}
					ariaLabel="Auto-commit"
					disabled={running}
					onchange={(on) => onmanualcommit?.(!on)}
				/>
			</span>
		{/if}

		{#if onsave}
			<span class="relative flex">
				<IconButton
					label={dirty ? `Save changes (${chord('S')})` : `Save query (${chord('S')})`}
					variant="quiet"
					size="sm"
					disabled={!canRun || saving || running}
					onclick={onsave}
				>
					{#if saving}<Spinner size="sm" />{:else}<Icon name="bookmark" />{/if}
				</IconButton>
				{#if dirty && !saving}
					<span
						class="pointer-events-none absolute top-1 right-1 size-1.5 rounded-full bg-warning ring-2 ring-surface"
						aria-hidden="true"
					></span>
				{/if}
			</span>
		{/if}

		<span class="mx-1 h-4 w-px bg-line" aria-hidden="true"></span>
		<Button
			variant={appPrefs.agentChatOpen ? 'secondary' : 'ghost'}
			size="sm"
			aria-pressed={appPrefs.agentChatOpen}
			title={appPrefs.agentChatOpen ? 'Close the agent' : 'Ask the agent to draft or fix a query'}
			onclick={() => appPrefs.setAgentChatOpen(!appPrefs.agentChatOpen)}
		>
			<span class="flex text-accent-text"><Icon name="sparkle" /></span>Agent
		</Button>
	</div>
</div>

{#if explainMenu}
	<ContextMenu
		x={explainMenu.x}
		y={explainMenu.y}
		items={EXPLAIN_ITEMS}
		ariaLabel="Explain"
		onpick={pickExplain}
		onclose={() => (explainMenu = null)}
	/>
{/if}
