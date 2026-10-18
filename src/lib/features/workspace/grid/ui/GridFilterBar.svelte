<script lang="ts">
	import { Icon, IconButton, Input, Select } from '#core/ui'
	import { opNeedsValue, opOptions } from '../filters'
	import type { FilterMode, FilterOp, FilterRule } from '../types'
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
		distinctValues?: string[]
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
		distinctValues = [],
	}: Props = $props()

	const distinctListId = $props.id()
</script>

<!-- Filters: fixed single-row height so UI ↔ code does not shift layout -->
<div class="shrink-0 border-b border-line bg-elevated px-2.5 py-2">
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
				<div class="min-w-0 flex-1">
					<Input
						inputSize="sm"
						class="font-mono"
						placeholder={opNeedsValue(rule.op) ? 'Value' : '—'}
						value={rule.value}
						disabled={!opNeedsValue(rule.op)}
						oninput={(e) =>
							onUpdateFirst({
								value: (e.currentTarget as HTMLInputElement).value,
							})}
						list={distinctValues.length > 0 ? distinctListId : undefined}
						onkeydown={(e) => {
							if (e.key === 'Enter') {
								e.preventDefault()
								onApply()
							}
						}}
					/>
				</div>
			{/if}
		{:else}
			<div class="relative min-w-0 flex-1">
				<Input
					inputSize="sm"
					class="font-mono"
					placeholder={codePlaceholder}
					bind:value={filterCode}
					oninput={(e) => {
						const el = e.currentTarget as HTMLInputElement
						onCodeInput?.(el.value, el.selectionStart ?? el.value.length)
					}}
					onkeydown={(e) => {
						if (e.key === 'Enter') {
							e.preventDefault()
							onApply()
						}
					}}
				/>
				{#if completions.length > 0}
					<ul
						class="absolute top-full left-0 z-30 mt-1 max-h-40 min-w-36 overflow-auto rounded-md border border-line bg-elevated py-1 shadow-sm"
					>
						{#each completions as item (item)}
							<li>
								<button
									type="button"
									class="block w-full px-2 py-1 text-left font-mono text-label text-fg hover:bg-hover"
									onclick={() => onPickCompletion?.(item)}
								>
									{item}
								</button>
							</li>
						{/each}
					</ul>
				{/if}
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
					<div class="min-w-0 flex-1">
						<Input
							inputSize="sm"
							class="font-mono"
							placeholder={opNeedsValue(rule.op) ? 'Value' : '—'}
							value={rule.value}
							disabled={!opNeedsValue(rule.op)}
							oninput={(e) =>
								onUpdateRule(rule.id, {
									value: (e.currentTarget as HTMLInputElement).value,
								})}
							list={distinctValues.length > 0 ? distinctListId : undefined}
							onkeydown={(e) => {
								if (e.key === 'Enter') {
									e.preventDefault()
									onApply()
								}
							}}
						/>
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
	{#if distinctValues.length > 0}
		<datalist id={distinctListId}>
			{#each distinctValues as value (value)}
				<option {value}></option>
			{/each}
		</datalist>
	{/if}
</div>
