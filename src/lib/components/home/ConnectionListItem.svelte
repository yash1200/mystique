<script lang="ts">
	import { Pill, StatusDot } from '$lib/components/core'
	import type { ConnectionProfile } from '$lib/types/connection'
	import {
		colorHex,
		connectionEndpoint,
		engineLabel,
	} from '$lib/types/connection'
	import { cn } from '$lib/utils/cn'

	const ENGINE_RANK: Record<string, string> = {
		postgres: 'Postgres',
		mysql: 'MySQL',
		mariadb: 'MariaDB',
		mongodb: 'MongoDB',
		sqlite: 'SQLite',
	}

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
	const engineRank = $derived(
		ENGINE_RANK[String(connection.engine).toLowerCase()] ??
			engineLabel(connection.engine),
	)
	const showStatus = $derived(
		connecting || status === 'connected' || status === 'error',
	)
</script>

<button
	type="button"
	class={cn(
		'group relative w-full min-h-row select-none rounded-md border py-2 pr-3 pl-3.5 text-left transition-[color,background-color,border-color] duration-150',
		'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus',
		selected
			? 'border-(--connection-color)/40 conn-tint'
			: 'border-transparent hover:bg-hover',
	)}
	style="--connection-color: {hex}"
	{onclick}
	{ondblclick}
	aria-current={selected ? 'true' : undefined}
	title="{engineLabel(
		connection.engine,
	)} · Click to select · Double-click to connect"
>
	<span
		class={cn(
			'absolute inset-y-2 left-0 w-stripe rounded-r-sm bg-conn',
			selected ? 'opacity-100' : 'opacity-70 group-hover:opacity-100',
		)}
		aria-hidden="true"
	></span>

	<div class="flex min-w-0 items-center gap-1.5">
		<span
			class={cn(
				'min-w-0 truncate text-body tracking-tight text-fg',
				selected ? 'font-semibold' : 'font-medium',
			)}
		>
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
		<span class="shrink-0 font-mono text-caption text-fg-secondary">
			{engineRank}
		</span>
		<span class="shrink-0 text-fg-muted" aria-hidden="true">·</span>
		<span class="min-w-0 truncate font-mono text-caption text-fg-muted">
			{#if connecting}
				Connecting…
			{:else}
				{endpoint}
			{/if}
		</span>
		{#if showStatus}
			<span class="ml-auto shrink-0">
				<StatusDot
					status={connecting ? 'running' : status}
					label={connecting ? 'Connecting' : undefined}
				/>
			</span>
		{/if}
	</div>
</button>
