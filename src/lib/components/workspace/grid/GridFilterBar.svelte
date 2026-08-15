<script lang="ts">
	import { IconButton, Input, Select } from '$lib/components/ui'
	import {
		type FilterMode,
		type FilterOp,
		type FilterRule,
		opNeedsValue,
		opOptions,
	} from './filterLogic'

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
	}: Props = $props()
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
			{#if filterMode === 'ui'}
				<svg
					class="size-3.5"
					viewBox="0 0 16 16"
					fill="none"
					aria-hidden="true"
				>
					<path
						d="M2.5 3.5h11l-4 4.5v3.5L6.5 13V8L2.5 3.5z"
						stroke="currentColor"
						stroke-width="1.3"
						stroke-linejoin="round"
					/>
				</svg>
			{:else}
				<svg
					class="size-3.5"
					viewBox="0 0 16 16"
					fill="none"
					aria-hidden="true"
				>
					<path
						d="M5.5 4.25 2.75 8 5.5 11.75M10.5 4.25 13.25 8 10.5 11.75M9 3.5l-2 9"
						stroke="currentColor"
						stroke-width="1.3"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				</svg>
			{/if}
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
			<div class="min-w-0 flex-1">
				<Input
					inputSize="sm"
					class="font-mono"
					placeholder={codePlaceholder}
					bind:value={filterCode}
					onkeydown={(e) => {
						if (e.key === 'Enter') {
							e.preventDefault()
							onApply()
						}
					}}
				/>
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
					<svg
						class="size-3.5"
						viewBox="0 0 16 16"
						fill="none"
						aria-hidden="true"
					>
						<path
							d="M4 4l8 8M12 4l-8 8"
							stroke="currentColor"
							stroke-width="1.4"
							stroke-linecap="round"
						/>
					</svg>
				</IconButton>
				<IconButton
					label="Add condition"
					size="sm"
					variant="ghost"
					onclick={onAddRule}
				>
					<svg
						class="size-3.5"
						viewBox="0 0 16 16"
						fill="none"
						aria-hidden="true"
					>
						<path
							d="M8 3.25v9.5M3.25 8h9.5"
							stroke="currentColor"
							stroke-width="1.4"
							stroke-linecap="round"
						/>
					</svg>
				</IconButton>
			{:else}
				<IconButton
					label="Clear filters"
					size="sm"
					variant="ghost"
					onclick={onClear}
				>
					<svg
						class="size-3.5"
						viewBox="0 0 16 16"
						fill="none"
						aria-hidden="true"
					>
						<path
							d="M4 4l8 8M12 4l-8 8"
							stroke="currentColor"
							stroke-width="1.4"
							stroke-linecap="round"
						/>
					</svg>
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
				<svg
					class="size-3.5"
					viewBox="0 0 16 16"
					fill="none"
					aria-hidden="true"
				>
					<circle
						cx="7"
						cy="7"
						r="4.25"
						stroke="currentColor"
						stroke-width="1.4"
					/>
					<path
						d="M10.25 10.25 13.5 13.5"
						stroke="currentColor"
						stroke-width="1.4"
						stroke-linecap="round"
					/>
				</svg>
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
							<svg
								class="size-3.5"
								viewBox="0 0 16 16"
								fill="none"
								aria-hidden="true"
							>
								<path
									d="M4 4l8 8M12 4l-8 8"
									stroke="currentColor"
									stroke-width="1.4"
									stroke-linecap="round"
								/>
							</svg>
						</IconButton>
						<span class="size-control-sm shrink-0" aria-hidden="true"></span>
						<span class="size-control-sm shrink-0" aria-hidden="true"></span>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>
