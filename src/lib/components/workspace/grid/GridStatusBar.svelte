<script lang="ts">
	import { Button, IconButton, Pager, StatusBar } from '$lib/components/core'
	import { cn } from '$lib/utils/cn'

	interface Props {
		filteredCount: number
		pageStart: number
		pageEnd: number
		filtersActive: boolean
		hasActiveFilters: boolean
		tableRowCount: number
		dirtyCount: number
		statusMsg: string | null
		safePage: number
		totalPages: number
		refreshing: boolean
		saving: boolean
		onPrev: () => void
		onNext: () => void
		onRefresh: () => void
		onAddRow: () => void
		onReset: () => void
		onSave: () => void
	}

	let {
		filteredCount,
		pageStart,
		pageEnd,
		filtersActive,
		hasActiveFilters,
		tableRowCount,
		dirtyCount,
		statusMsg,
		safePage,
		totalPages,
		refreshing,
		saving,
		onPrev,
		onNext,
		onRefresh,
		onAddRow,
		onReset,
		onSave,
	}: Props = $props()

	const hasChanges = $derived(dirtyCount > 0)
</script>

<StatusBar ariaLabel="Table status">
	{#snippet left()}
		{#if filteredCount === 0}
			<span class="truncate font-mono tabular-nums">0 rows</span>
		{:else}
			<span class="truncate font-mono tabular-nums">
				{pageStart + 1}–{pageEnd} of
				{#if filtersActive && hasActiveFilters}
					{filteredCount} filtered · {tableRowCount.toLocaleString()} total
				{:else}
					{tableRowCount.toLocaleString()}
				{/if}
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
			variant="ghost"
			disabled={refreshing}
			aria-busy={refreshing}
			onclick={onRefresh}
			class={refreshing ? 'opacity-45' : ''}
		>
			<svg
				class={cn('size-3.5', refreshing && 'animate-spin')}
				viewBox="0 0 16 16"
				fill="none"
				aria-hidden="true"
			>
				<path
					d="M13.5 8A5.5 5.5 0 1 1 11.2 3.4"
					stroke="currentColor"
					stroke-width="1.4"
					stroke-linecap="round"
				/>
				<path
					d="M13.5 2.5v3.2h-3.2"
					stroke="currentColor"
					stroke-width="1.4"
					stroke-linecap="round"
					stroke-linejoin="round"
				/>
			</svg>
		</IconButton>
		<IconButton
			label="Add a new empty row (⌘/Ctrl+N)"
			size="sm"
			variant="ghost"
			disabled={saving || refreshing}
			onclick={onAddRow}
		>
			<svg class="size-3.5" viewBox="0 0 16 16" fill="none" aria-hidden="true">
				<path
					d="M8 3.25v9.5M3.25 8h9.5"
					stroke="currentColor"
					stroke-width="1.4"
					stroke-linecap="round"
				/>
			</svg>
		</IconButton>
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
