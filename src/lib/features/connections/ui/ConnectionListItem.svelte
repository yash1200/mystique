<script lang="ts">
	import type { ConnectionProfile } from '#core/contracts'
	import { Icon, Pill, StatusDot } from '#core/ui'
	import { cn } from '#core/util'
	import { engineLabel } from '#engines'
	import { colorHex, connectionEndpoint, connectionInitial } from '../model'

	const ENGINE_RANK: Record<string, string> = {
		postgres: 'Postgres',
		mysql: 'MySQL',
		mariadb: 'MariaDB',
		mongodb: 'MongoDB',
		sqlite: 'SQLite',
	}

	interface Props {
		connection: ConnectionProfile
		selected: boolean
		connecting: boolean
		query: string
		onclick: () => void
		ondblclick: () => void
		oncontextmenu: (e: MouseEvent) => void
		onfavorite: () => void
	}

	let {
		connection,
		selected,
		connecting,
		query,
		onclick,
		ondblclick,
		oncontextmenu,
		onfavorite,
	}: Props = $props()

	const hex = $derived(colorHex(connection.colorId))
	const status = $derived(connection.status ?? 'disconnected')
	const host = $derived.by(() => {
		const raw = connectionEndpoint(connection).trim()
		if (!raw || raw === '—') return ''
		return raw
	})
	const engineRank = $derived(
		ENGINE_RANK[String(connection.engine).toLowerCase()] ??
			engineLabel(connection.engine),
	)
	const showStatus = $derived(
		connecting || status === 'connected' || status === 'error',
	)
	const initial = $derived(
		connectionInitial(connection.name, String(connection.engine)),
	)

	function matchParts(
		text: string,
		q: string,
	): { text: string; hit: boolean }[] {
		const needle = q.trim()
		if (!needle) return [{ text, hit: false }]
		const i = text.toLowerCase().indexOf(needle.toLowerCase())
		if (i < 0) return [{ text, hit: false }]
		return [
			{ text: text.slice(0, i), hit: false },
			{ text: text.slice(i, i + needle.length), hit: true },
			{ text: text.slice(i + needle.length), hit: false },
		].filter((part) => part.text.length > 0)
	}
</script>

<div
	class={cn(
		'group relative w-full min-h-row select-none rounded-lg border',
		'transition-[color,background-color,border-color] duration-150 ease-out',
		selected
			? 'border-(--connection-color)/35 conn-tint'
			: 'border-transparent hover:bg-hover',
	)}
	style="--connection-color: {hex}"
>
	<span
		class={cn(
			'absolute inset-y-1.5 left-0 w-stripe rounded-r-sm bg-conn transition-opacity duration-150',
			selected ? 'opacity-100' : 'opacity-55 group-hover:opacity-100',
		)}
		aria-hidden="true"
	></span>

	<div class="flex min-w-0 items-center">
		<button
			type="button"
			class="flex min-w-0 flex-1 items-center gap-2 border-none bg-transparent py-2 pr-1 pl-3 text-left transition-transform duration-150 enabled:active:scale-[0.99] focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus"
			{onclick}
			{ondblclick}
			{oncontextmenu}
			aria-current={selected ? 'true' : undefined}
			aria-busy={connecting || undefined}
			title="{engineLabel(
				connection.engine,
			)} · Click to select · Double-click to connect · Right-click for actions"
		>
			<span
				class={cn(
					'relative grid size-7 shrink-0 place-items-center rounded-md text-caption font-semibold leading-none transition-colors duration-150',
					selected
						? 'bg-(--connection-color)/20'
						: 'bg-input group-hover:bg-(--connection-color)/12',
				)}
				style="color: var(--connection-color)"
			>
				<span aria-hidden="true">{initial}</span>
				{#if showStatus}
					<span
						class="absolute -right-0.5 -bottom-0.5 grid size-status place-items-center rounded-full ring-2 ring-panel"
					>
						<StatusDot
							status={connecting ? 'running' : status}
							label={connecting ? 'Connecting' : undefined}
							pulse={false}
						/>
					</span>
				{/if}
			</span>

			<div class="min-w-0 flex-1">
				<div class="flex min-w-0 items-center gap-1.5">
					<span
						class={cn(
							'min-w-0 truncate text-body tracking-tight text-fg',
							selected ? 'font-semibold' : 'font-medium',
						)}
					>
						{#each matchParts(connection.name, query) as part, i (i)}
							{#if part.hit}
								<mark class="rounded-sm bg-accent-muted p-0 text-fg"
									>{part.text}</mark
								>
							{:else}
								{part.text}
							{/if}
						{/each}
					</span>
					{#if connection.readOnly}
						<Pill uppercase>RO</Pill>
					{/if}
				</div>

				<div class="mt-0.5 flex min-w-0 items-center gap-2">
					<span class="shrink-0 font-mono text-caption text-fg-secondary">
						{engineRank}
					</span>
					{#if connecting}
						<span class="min-w-0 truncate font-mono text-caption text-fg-muted">
							Connecting…
						</span>
					{:else if host}
						<span class="min-w-0 truncate font-mono text-caption text-fg-muted">
							{host}
						</span>
					{/if}
				</div>
			</div>
		</button>

		<div class="flex shrink-0 items-center pr-2">
			<button
				type="button"
				class={cn(
					'grid size-6 shrink-0 place-items-center rounded-sm border-none bg-transparent transition-colors',
					connection.favorite
						? 'text-accent'
						: 'text-fg-muted opacity-0 hover:text-fg group-hover:opacity-100',
					'focus-visible:opacity-100',
				)}
				aria-label={connection.favorite
					? 'Remove from favourites'
					: 'Add to favourites'}
				aria-pressed={connection.favorite}
				onclick={(e) => {
					e.stopPropagation()
					onfavorite()
				}}
				{oncontextmenu}
			>
				<Icon
					name={connection.favorite ? 'star-filled' : 'star'}
					class="size-3.5"
				/>
			</button>
		</div>
	</div>
</div>
