<script lang="ts">
	import { EngineBadge, Pill, StatusDot } from '$lib/components/ui'
	import type { ConnectionProfile } from '$lib/types/connection'
	import {
		colorHex,
		connectionEndpoint,
		engineLabel,
	} from '$lib/types/connection'
	import { cn } from '$lib/utils/cn'

	interface Props {
		connection: ConnectionProfile
		selected?: boolean
		connecting?: boolean
		onclick?: () => void
		ondblclick?: () => void
	}

	let {
		connection,
		selected = false,
		connecting = false,
		onclick,
		ondblclick,
	}: Props = $props()

	const hex = $derived(colorHex(connection.colorId))
	const status = $derived(connection.status ?? 'disconnected')
	const endpoint = $derived(connectionEndpoint(connection))
	const prod = $derived(connection.environment === 'production')
</script>

<button
	type="button"
	class={cn(
		'group relative w-full select-none rounded-lg border py-2.5 pr-3 pl-0 text-left transition-[color,background-color,border-color] duration-150',
		selected
			? 'border-(--connection-color)/40 conn-tint'
			: prod
				? 'border-transparent bg-warning/10 hover:border-warning/30 hover:bg-warning/15'
				: 'border-transparent bg-transparent hover:border-line hover:bg-hover',
	)}
	style="--connection-color: {hex}"
	{onclick}
	{ondblclick}
	aria-current={selected ? 'true' : undefined}
	title="Click to select · Double-click to connect"
>
	<span
		class={cn(
			'absolute inset-y-1.5 left-0 w-1 rounded-r-sm bg-conn',
			selected ? 'opacity-100' : 'opacity-70 group-hover:opacity-100',
		)}
		aria-hidden="true"
	></span>
	<div class="flex min-w-0 items-center gap-2.5 pl-3">
		<div
			class="grid size-8 shrink-0 place-items-center rounded-md border border-line bg-elevated"
		>
			<EngineBadge engine={connection.engine} size="sm" />
		</div>
		<div class="min-w-0 flex-1">
			<div class="flex min-w-0 items-center gap-1.5">
				<span class="truncate text-body font-semibold tracking-tight text-fg">
					{connection.name}
				</span>
				{#if prod}
					<Pill tone="warning" uppercase>PROD</Pill>
				{/if}
				{#if connection.readOnly}
					<Pill uppercase>RO</Pill>
				{/if}
			</div>
			<div class="mt-0.5 flex min-w-0 items-center gap-1.5">
				<StatusDot {status} />
				<span class="truncate font-mono text-caption text-fg-muted">
					{#if connecting}
						Connecting…
					{:else}
						{endpoint}
					{/if}
				</span>
			</div>
		</div>
		<span
			class="shrink-0 font-mono text-micro tracking-wide text-fg-muted uppercase"
		>
			{engineLabel(connection.engine)}
		</span>
	</div>
</button>
