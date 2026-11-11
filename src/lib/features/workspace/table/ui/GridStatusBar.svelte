<script lang="ts">
	import {
		Button,
		FetchedAgo,
		Icon,
		IconButton,
		Pager,
		StatusBar,
		ViewLinks,
		type ViewLink,
	} from '#core/ui'
	import { cn } from '#core/util'
	import { StatusNote } from '#features/toast'
	interface Props {
		dirtyCount: number
		/** Epoch ms of the last engine preview fetch. */
		lastFetchedAt: number | null
		safePage: number
		totalPages: number
		refreshing: boolean
		saving: boolean
		readOnly?: boolean
		onOpenStructure?: () => void
		onOpenErd?: () => void
		onPrev: () => void
		onNext: () => void
		onRefresh: () => void
		onAddRow: () => void
		onReset: () => void
		onSave: () => void
		selectionLabel?: string
		canUndo?: boolean
		canRedo?: boolean
		canRollback?: boolean
		onUndo?: () => void
		onRedo?: () => void
		onRollback?: () => void
		/** Live "Show SQL" side panel. */
		sqlPanelOpen?: boolean
		onToggleSql?: () => void
		/** Background total count (`countLabel`). */
		count?: { text: string; title: string; action: 'cancel' | 'count' | null } | null
		/** Count every row (shown when the total is estimated or unknown). */
		onCountExactly?: () => void
		onCancelCount?: () => void
	}

	let {
		dirtyCount,
		lastFetchedAt,
		safePage,
		totalPages,
		refreshing,
		saving,
		readOnly = false,
		onOpenStructure,
		onOpenErd,
		onPrev,
		onNext,
		onRefresh,
		onAddRow,
		onReset,
		onSave,
		selectionLabel = '',
		canUndo = false,
		canRedo = false,
		canRollback = false,
		onUndo,
		onRedo,
		onRollback,
		sqlPanelOpen = false,
		onToggleSql,
		count = null,
		onCountExactly,
		onCancelCount,
	}: Props = $props()

	const hasChanges = $derived(dirtyCount > 0)

	const views = $derived.by(() => {
		const out: ViewLink[] = []
		if (onOpenStructure) {
			out.push({ label: 'Structure', icon: 'structure', title: 'Open the table structure', onclick: onOpenStructure })
		}
		if (onOpenErd) {
			out.push({ label: 'ERD', icon: 'schema', title: 'Open the ER diagram', onclick: onOpenErd })
		}
		return out
	})
</script>

<StatusBar class={views.length ? 'pl-1.5!' : ''} ariaLabel="Table status">
	{#snippet left()}
		<ViewLinks links={views} />
		{#if hasChanges}
			<span class="inline-flex shrink-0 items-center gap-[7px] text-warning">
				<span class="size-[7px] rounded-full bg-warning" aria-hidden="true"></span>
				{dirtyCount} unsaved {dirtyCount === 1 ? 'change' : 'changes'}
			</span>
		{/if}
		{#if count}
			{#if count.action === 'count' && onCountExactly}
				<button
					type="button"
					class="shrink-0 rounded-md border-none bg-transparent px-1 py-0.5 font-mono text-label text-fg-muted tabular-nums underline decoration-line-strong decoration-dotted underline-offset-4 transition-colors hover:text-fg"
					title={count.title}
					onclick={onCountExactly}
				>
					{count.text}
				</button>
			{:else}
				<span
					class="inline-flex shrink-0 items-center gap-0.5 font-mono tabular-nums"
					title={count.title}
					aria-live="polite"
				>
					{count.text}
					{#if count.action === 'cancel' && onCancelCount}
						<IconButton label="Cancel the row count" size="sm" variant="quiet" onclick={onCancelCount}>
							<Icon name="close" class="size-3" />
						</IconButton>
					{/if}
				</span>
			{/if}
		{/if}
		{#if selectionLabel}
			<span class="min-w-0 truncate font-mono tabular-nums">{selectionLabel}</span>
		{/if}
		<StatusNote />
	{/snippet}
	{#snippet center()}
		<Pager
			page={safePage}
			{totalPages}
			disabled={refreshing || saving}
			onprev={onPrev}
			onnext={onNext}
		/>
	{/snippet}
	{#snippet right()}
		{#if lastFetchedAt != null}
			<FetchedAgo at={lastFetchedAt} class="mr-1" />
		{/if}
		<IconButton
			label={refreshing ? 'Refreshing table' : 'Refresh table (⌘/Ctrl+R)'}
			size="sm"
			variant="quiet"
			disabled={refreshing}
			aria-busy={refreshing}
			onclick={onRefresh}
		>
			<Icon name="refresh" class={cn(refreshing && 'animate-spin')} />
		</IconButton>
		{#if !readOnly}
			<IconButton
				label="Add a new empty row (⌘/Ctrl+N)"
				size="sm"
				variant="quiet"
				disabled={saving || refreshing}
				onclick={onAddRow}
			>
				<Icon name="plus" />
			</IconButton>
		{/if}
		{#if onToggleSql && !readOnly}
			<IconButton
				label={sqlPanelOpen ? 'Hide pending SQL' : 'Show pending SQL'}
				size="sm"
				variant="quiet"
				active={sqlPanelOpen}
				onclick={onToggleSql}
			>
				<Icon name="code" />
			</IconButton>
		{/if}
		{#if (canUndo && onUndo) || (canRedo && onRedo) || (canRollback && onRollback) || hasChanges}
			<span class="mx-1 h-4 w-px shrink-0 bg-line" aria-hidden="true"></span>
		{/if}
		{#if canUndo && onUndo}
			<Button variant="ghost" size="sm" disabled={saving} onclick={onUndo} title="Undo pending edit">Undo</Button>
		{/if}
		{#if canRedo && onRedo}
			<Button variant="ghost" size="sm" disabled={saving} onclick={onRedo} title="Redo pending edit">Redo</Button>
		{/if}
		{#if canRollback && onRollback}
			<Button
				variant="ghost"
				size="sm"
				disabled={saving || refreshing}
				onclick={onRollback}
				title="Roll back the last save"
			>
				Rollback
			</Button>
		{/if}
		{#if hasChanges}
			<Button variant="ghost" size="sm" disabled={saving} onclick={onReset} title="Discard unsaved changes">
				Reset
			</Button>
			<Button variant="primary" size="sm" disabled={saving} onclick={onSave} title="Save changes (⌘/Ctrl+S)">
				{saving ? 'Saving…' : 'Save'}
			</Button>
		{/if}
	{/snippet}
</StatusBar>
