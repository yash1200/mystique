<script lang="ts">
	import {
		Button,
		Icon,
		IconButton,
		Pager,
		StatusBar,
	} from '$lib/components/core'
	import { cn } from '$lib/utils/cn'
	import { fetchedLabel } from '$lib/utils/fetchedLabel'

	interface Props {
		filteredCount: number
		filtersActive: boolean
		hasActiveFilters: boolean
		dirtyCount: number
		statusMsg: string | null
		/** Epoch ms of the last engine preview fetch. */
		lastFetchedAt: number | null
		safePage: number
		totalPages: number
		refreshing: boolean
		saving: boolean
		readOnly?: boolean
		onOpenStructure?: () => void
		onPrev: () => void
		onNext: () => void
		onRefresh: () => void
		onAddRow: () => void
		onReset: () => void
		onSave: () => void
	}

	let {
		filteredCount,
		filtersActive,
		hasActiveFilters,
		dirtyCount,
		statusMsg,
		lastFetchedAt,
		safePage,
		totalPages,
		refreshing,
		saving,
		readOnly = false,
		onOpenStructure,
		onPrev,
		onNext,
		onRefresh,
		onAddRow,
		onReset,
		onSave,
	}: Props = $props()

	const hasChanges = $derived(dirtyCount > 0)
	let now = $state(Date.now())

	$effect(() => {
		const id = window.setInterval(() => {
			now = Date.now()
		}, 15_000)
		return () => window.clearInterval(id)
	})
</script>

<StatusBar class="!pl-1" ariaLabel="Table status">
	{#snippet left()}
		{#if onOpenStructure}
			<Button
				variant="ghost"
				size="sm"
				class="!px-1.5"
				onclick={onOpenStructure}
				title="Open table structure in a new tab"
			>
				<Icon name="structure" class="size-3.5" />
				Structure
			</Button>
		{/if}
		{#if lastFetchedAt != null}
			<span
				class="inline-flex min-w-0 items-center gap-1 truncate text-caption text-fg-muted"
				title={new Date(lastFetchedAt).toLocaleString()}
			>
				<Icon name="history" class="size-3.5 shrink-0" />
				<span class="truncate tabular-nums">{fetchedLabel(lastFetchedAt, now)}</span>
			</span>
		{/if}
		{#if filteredCount === 0}
			<span class="truncate font-mono tabular-nums text-fg-muted">0 rows</span>
		{:else if filtersActive && hasActiveFilters}
			<span class="truncate font-mono tabular-nums text-fg-muted">
				{filteredCount} filtered
			</span>
		{/if}
		{#if hasChanges}
			<span
				class="shrink-0 rounded-sm border border-warning/45 bg-warning/15 px-1 py-px text-micro font-medium text-warning"
			>
				{dirtyCount} unsaved
			</span>
		{/if}
		{#if statusMsg}
			<span class="shrink-0 truncate text-success">{statusMsg}</span>
		{/if}
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
		<IconButton
			label={refreshing ? 'Refreshing table' : 'Refresh table (⌘/Ctrl+R)'}
			size="sm"
			variant="quiet"
			disabled={refreshing}
			aria-busy={refreshing}
			onclick={onRefresh}
			class={refreshing ? 'opacity-45' : ''}
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
		{#if hasChanges}
			<Button
				variant="ghost"
				size="sm"
				disabled={saving}
				onclick={onReset}
				title="Discard unsaved changes"
			>
				Reset
			</Button>
			<Button
				variant="primary"
				size="sm"
				disabled={saving}
				onclick={onSave}
				title="Save changes (⌘/Ctrl+S)"
			>
				{saving ? 'Saving…' : 'Save'}
			</Button>
		{/if}
	{/snippet}
</StatusBar>
