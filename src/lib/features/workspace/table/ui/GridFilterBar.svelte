<script lang="ts">
	import { Icon, IconButton, Input, Select } from '#core/ui'
	import { opNeedsValue, opOptions } from '../filters/filters'
	import type { FilterMode, FilterOp, FilterRule } from '../filters/types'
	interface Props {
		filterMode: FilterMode
		filterRules: FilterRule[]
		filterCode?: string
		columnOptions: { value: string; label: string }[]
		onSetMode: (mode: FilterMode) => void
		onUpdateFirst: (patch: Partial<FilterRule>) => void
		onUpdateRule: (id: string, patch: Partial<FilterRule>) => void
		onRemoveRule: (id: string) => void
		onAddRule: () => void
		onApply: () => void
		onClear: () => void
		codePlaceholder?: string
		completions?: string[]
		onCodeInput?: (value: string, caret: number) => void
		onPickCompletion?: (item: string) => void
		/** Distinct values for the field named by `valueTarget` (a rule id or `code`). */
		valueSuggestions?: string[]
		valueTarget?: string | null
		onValueInput?: (ruleId: string) => void
		/** Returns the code-filter caret after the insert, if any. */
		onPickValue?: (value: string) => number | null
		onCloseValues?: () => void
	}

	let {
		filterMode,
		filterRules,
		filterCode = $bindable(''),
		columnOptions,
		codePlaceholder = "status = 'pending' AND total_cents > 1000",
		onSetMode,
		onUpdateFirst,
		onUpdateRule,
		onRemoveRule,
		onAddRule,
		onApply,
		onClear,
		completions = [],
		onCodeInput,
		onPickCompletion,
		valueSuggestions = [],
		valueTarget = null,
		onValueInput,
		onPickValue,
		onCloseValues,
	}: Props = $props()

	const listId = $props.id()
	let codeWrap = $state<HTMLDivElement | null>(null)
	/** Highlighted suggestion; -1 is none (Enter applies the filter). */
	let active = $state(-1)

	const codeItems = $derived(
		completions.length > 0 ? completions : valueTarget === 'code' ? valueSuggestions : [],
	)

	$effect(() => {
		void valueSuggestions
		void completions
		active = -1
	})

	function listFor(target: string): string[] {
		return target === 'code' ? codeItems : valueTarget === target ? valueSuggestions : []
	}

	function pick(target: string, item: string) {
		active = -1
		if (target === 'code' && completions.length > 0) {
			onPickCompletion?.(item)
			return
		}
		const caret = onPickValue?.(item) ?? null
		if (target === 'code' && caret != null) {
			queueMicrotask(() => {
				const input = codeWrap?.querySelector('input')
				input?.focus()
				input?.setSelectionRange(caret, caret)
			})
		}
	}

	/** Arrow keys walk the list, Enter picks or applies, Escape closes. */
	function onFieldKeydown(e: KeyboardEvent, target: string) {
		const items = listFor(target)
		if (items.length > 0) {
			if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
				e.preventDefault()
				const step = e.key === 'ArrowDown' ? 1 : -1
				active = Math.max(-1, Math.min(items.length - 1, active + step))
				return
			}
			if (e.key === 'Escape') {
				e.preventDefault()
				e.stopPropagation()
				onCloseValues?.()
				return
			}
			if ((e.key === 'Enter' || e.key === 'Tab') && active >= 0) {
				const item = items[active]
				if (item != null) {
					e.preventDefault()
					pick(target, item)
					return
				}
			}
		}
		if (e.key === 'Enter') {
			e.preventDefault()
			onCloseValues?.()
			onApply()
		}
	}
</script>

{#snippet suggestions(target: string)}
	{@const items = listFor(target)}
	{#if items.length > 0}
		<ul
			id="{listId}-{target}"
			role="listbox"
			aria-label={target === 'code' && completions.length > 0 ? 'Columns' : 'Values'}
			class="absolute top-full left-0 z-30 mt-1 max-h-48 max-w-full min-w-36 overflow-auto rounded-md border border-line bg-raised py-1 shadow-sm"
		>
			{#each items as item, i (item)}
				<li role="option" aria-selected={i === active}>
					<button
						type="button"
						tabindex="-1"
						class="block w-full truncate px-2 py-1 text-left font-mono text-label text-fg hover:bg-hover {i ===
						active
							? 'bg-hover'
							: ''}"
						onmousedown={(e) => e.preventDefault()}
						onclick={() => pick(target, item)}
					>
						{item}
					</button>
				</li>
			{/each}
		</ul>
	{/if}
{/snippet}

<!-- Filters: fixed single-row height so UI ↔ code does not shift layout -->
<div class="shrink-0 border-b border-line bg-raised px-2.5 py-2">
	<div class="flex w-full min-w-0 items-center gap-1.5">
		<!-- Mode switch (funnel = UI, code = expression) -->
		<IconButton
			label={filterMode === 'ui'
				? 'Switch to code filters'
				: 'Switch to UI filters'}
			size="sm"
			variant="ghost"
			active={filterMode === 'code'}
			onclick={() => onSetMode(filterMode === 'ui' ? 'code' : 'ui')}
		>
			<Icon name={filterMode === 'ui' ? 'funnel' : 'code-slash'} />
		</IconButton>

		{#if filterMode === 'ui'}
			{@const rule = filterRules[0]}
			{#if rule}
				<div class="w-[15%] min-w-0 shrink-0">
					<Select
						value={rule.column}
						options={columnOptions}
						inputSize="sm"
						placeholder="Column"
						onchange={(v) => onUpdateFirst({ column: v })}
					/>
				</div>
				<div class="w-[15%] min-w-0 shrink-0">
					<Select
						value={rule.op}
						options={opOptions}
						inputSize="sm"
						placeholder="Operator"
						onchange={(v) => onUpdateFirst({ op: v as FilterOp })}
					/>
				</div>
				<div class="relative min-w-0 flex-1">
					<Input
						inputSize="sm"
						class="font-mono"
						placeholder={opNeedsValue(rule.op) ? 'Value' : '—'}
						value={rule.value}
						disabled={!opNeedsValue(rule.op)}
						role="combobox"
						aria-expanded={listFor(rule.id).length > 0}
						aria-controls="{listId}-{rule.id}"
						aria-autocomplete="list"
						oninput={(e) => {
							onUpdateFirst({
								value: (e.currentTarget as HTMLInputElement).value,
							})
							onValueInput?.(rule.id)
						}}
						onfocus={() => onValueInput?.(rule.id)}
						onblur={() => onCloseValues?.()}
						onkeydown={(e) => onFieldKeydown(e, rule.id)}
					/>
					{@render suggestions(rule.id)}
				</div>
			{/if}
		{:else}
			<div class="relative min-w-0 flex-1" bind:this={codeWrap}>
				<Input
					inputSize="sm"
					class="font-mono"
					placeholder={codePlaceholder}
					bind:value={filterCode}
					role="combobox"
					aria-expanded={codeItems.length > 0}
					aria-controls="{listId}-code"
					aria-autocomplete="list"
					oninput={(e) => {
						const el = e.currentTarget as HTMLInputElement
						onCodeInput?.(el.value, el.selectionStart ?? el.value.length)
					}}
					onblur={() => onCloseValues?.()}
					onkeydown={(e) => onFieldKeydown(e, 'code')}
				/>
				{@render suggestions('code')}
			</div>
		{/if}

		<!-- Trailing actions: same three slots in both modes (stable width/height) -->
		<div class="flex h-control-sm shrink-0 items-center gap-1">
			{#if filterMode === 'ui'}
				<IconButton
					label="Remove condition"
					size="sm"
					variant="ghost"
					disabled={filterRules.length <= 1}
					onclick={() => {
						if (filterRules[0]) onRemoveRule(filterRules[0].id)
					}}
				>
					<Icon name="close" />
				</IconButton>
				<IconButton
					label="Add condition"
					size="sm"
					variant="ghost"
					onclick={onAddRule}
				>
					<Icon name="plus" />
				</IconButton>
			{:else}
				<IconButton
					label="Clear filters"
					size="sm"
					variant="ghost"
					onclick={onClear}
				>
					<Icon name="close" />
				</IconButton>
				<!-- Placeholder keeps action group width identical to UI mode -->
				<span class="size-control-sm shrink-0" aria-hidden="true"></span>
			{/if}
			<IconButton
				label="Apply filters"
				size="sm"
				variant="accent"
				active
				onclick={onApply}
			>
				<Icon name="search" />
			</IconButton>
		</div>
	</div>

	<!-- Extra UI conditions (below first row; does not affect code/UI toggle height) -->
	{#if filterMode === 'ui' && filterRules.length > 1}
		<div class="mt-1.5 flex flex-col gap-1.5">
			{#each filterRules.slice(1) as rule (rule.id)}
				<div class="flex w-full min-w-0 items-center gap-1.5">
					<span class="size-control-sm shrink-0" aria-hidden="true"></span>
					<div class="w-[15%] min-w-0 shrink-0">
						<Select
							value={rule.column}
							options={columnOptions}
							inputSize="sm"
							placeholder="Column"
							onchange={(v) => onUpdateRule(rule.id, { column: v })}
						/>
					</div>
					<div class="w-[15%] min-w-0 shrink-0">
						<Select
							value={rule.op}
							options={opOptions}
							inputSize="sm"
							placeholder="Operator"
							onchange={(v) => onUpdateRule(rule.id, { op: v as FilterOp })}
						/>
					</div>
					<div class="relative min-w-0 flex-1">
						<Input
							inputSize="sm"
							class="font-mono"
							placeholder={opNeedsValue(rule.op) ? 'Value' : '—'}
							value={rule.value}
							disabled={!opNeedsValue(rule.op)}
							role="combobox"
							aria-expanded={listFor(rule.id).length > 0}
							aria-controls="{listId}-{rule.id}"
							aria-autocomplete="list"
							oninput={(e) => {
								onUpdateRule(rule.id, {
									value: (e.currentTarget as HTMLInputElement).value,
								})
								onValueInput?.(rule.id)
							}}
							onfocus={() => onValueInput?.(rule.id)}
							onblur={() => onCloseValues?.()}
							onkeydown={(e) => onFieldKeydown(e, rule.id)}
						/>
						{@render suggestions(rule.id)}
					</div>
					<div class="flex h-control-sm shrink-0 items-center gap-1">
						<IconButton
							label="Remove condition"
							size="sm"
							variant="ghost"
							onclick={() => onRemoveRule(rule.id)}
						>
							<Icon name="close" />
						</IconButton>
						<span class="size-control-sm shrink-0" aria-hidden="true"></span>
						<span class="size-control-sm shrink-0" aria-hidden="true"></span>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>
