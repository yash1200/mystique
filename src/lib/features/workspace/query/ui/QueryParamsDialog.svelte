<script lang="ts">
	import { Button, Input, Modal } from '#core/ui'
	import {
		encodeParam,
		paramHistory,
		rememberParamValue,
		type ParamKind,
		type ParamMemory,
		type ParamSlot,
	} from '../params'

	interface Props {
		open: boolean
		slots: ParamSlot[]
		onconfirm: (values: unknown[]) => void
		oncancel: () => void
	}

	type Row = {
		label: string
		kind: ParamKind
		raw: string
		touched: boolean
		recent: ParamMemory[]
	}

	let { open, slots, onconfirm, oncancel }: Props = $props()

	let rows = $state<Row[]>([])

	$effect(() => {
		if (!open) return
		rows = slots.map((slot) => {
			const recent = paramHistory(slot.label)
			const last = recent[0]
			if (!last) {
				return {
					label: slot.label,
					kind: 'text' as const,
					raw: '',
					touched: false,
					recent,
				}
			}
			return {
				label: slot.label,
				kind: last.kind,
				raw: last.raw,
				touched: true,
				recent,
			}
		})
	})

	const ready = $derived(
		rows.length > 0 &&
			rows.every(
				(row) => encodeParam(row.kind, row.raw, row.touched) !== undefined,
			),
	)

	function setKind(index: number, kind: ParamKind) {
		const row = rows[index]
		if (!row) return
		row.kind = kind
		row.touched = true
		if (kind === 'bool' && row.raw !== 'false') row.raw = 'true'
		if (kind === 'null') row.raw = ''
	}

	function applyRecent(index: number, memory: ParamMemory) {
		const row = rows[index]
		if (!row) return
		row.kind = memory.kind
		row.raw = memory.raw
		row.touched = true
	}

	function submit() {
		if (!ready) return
		const values: unknown[] = []
		for (const row of rows) {
			const value = encodeParam(row.kind, row.raw, row.touched)
			if (value === undefined) return
			values.push(value)
			rememberParamValue(row.label, {
				kind: row.kind,
				raw: row.kind === 'null' ? '' : row.kind === 'bool' ? (value ? 'true' : 'false') : row.raw,
			})
		}
		onconfirm(values)
	}
</script>

<Modal
	{open}
	onclose={oncancel}
	title="Query parameters"
	description="Values are bound. They are not pasted into the SQL."
	size="md"
	onkeydown={(e) => {
		if (e.key === 'Enter') submit()
	}}
>
	<div class="mt-3 flex flex-col gap-3">
		{#each rows as row, index (row.label + index)}
			<div class="flex flex-col gap-1.5">
				<div class="flex items-center gap-2">
					<label
						class="min-w-0 flex-1 truncate font-mono text-label text-fg"
						for="param-{index}"
					>
						{row.label}
					</label>
					<select
						class="h-control-sm rounded-md border border-line bg-input px-2 text-label text-fg"
						value={row.kind}
						onchange={(event) => {
							const next = event.currentTarget.value
							if (
								next === 'text' ||
								next === 'number' ||
								next === 'bool' ||
								next === 'null'
							) {
								setKind(index, next)
							}
						}}
						aria-label="Type for {row.label}"
					>
						<option value="text">Text</option>
						<option value="number">Number</option>
						<option value="bool">Boolean</option>
						<option value="null">Null</option>
					</select>
				</div>
				{#if row.kind === 'bool'}
					<select
						id="param-{index}"
						class="h-control-sm rounded-md border border-line bg-input px-2 text-label text-fg"
						value={row.raw === 'false' ? 'false' : 'true'}
						onchange={(event) => {
							row.raw = event.currentTarget.value
							row.touched = true
						}}
					>
						<option value="true">true</option>
						<option value="false">false</option>
					</select>
				{:else if row.kind === 'null'}
					<p id="param-{index}" class="m-0 font-mono text-caption text-fg-muted">
						NULL
					</p>
				{:else}
					<Input
						id="param-{index}"
						bind:value={row.raw}
						inputSize="sm"
						placeholder={row.kind === 'number' ? '0' : 'value'}
						oninput={() => {
							row.touched = true
						}}
					/>
				{/if}
				{#if row.recent.length > 0}
					<div class="flex flex-wrap gap-1">
						{#each row.recent as memory, recentIndex (`${memory.kind}:${memory.raw}:${recentIndex}`)}
							<button
								type="button"
								class="rounded-md border border-line bg-elevated px-1.5 py-0.5 font-mono text-caption text-fg-secondary"
								onclick={() => applyRecent(index, memory)}
							>
								{memory.kind === 'null'
									? 'null'
									: memory.raw === ''
										? 'empty'
										: memory.raw}
							</button>
						{/each}
					</div>
				{/if}
			</div>
		{/each}
	</div>

	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={oncancel}>Cancel</Button>
		<Button variant="primary" size="sm" disabled={!ready} onclick={submit}>
			Run
		</Button>
	{/snippet}
</Modal>
