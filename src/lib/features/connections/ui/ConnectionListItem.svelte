<script lang="ts">
	import { Icon, Pill, StatusDot } from '#core/ui'
	import { cn } from '#core/util'
	import { engineLabel } from '#engines'
	import { connectionTarget } from '../facts'
	import type { HomeConnection } from '../home.svelte'
	import { colorHex } from '../model'
	import ConnectionTile from './ConnectionTile.svelte'

	interface Props {
		connection: HomeConnection
		selected: boolean
		connecting: boolean
		/** Highlights the filter match in the name. */
		query: string
		onselect: () => void
		onopen: () => void
		oncontextmenu: (e: MouseEvent) => void
		onfavorite: () => void
	}

	let {
		connection,
		selected,
		connecting,
		query,
		onselect,
		onopen,
		oncontextmenu,
		onfavorite,
	}: Props = $props()

	const engine = $derived(engineLabel(connection.engine))
	const target = $derived(connectionTarget(connection))

	/** The name split around the first match of the filter. */
	const nameParts = $derived.by(() => {
		const name = connection.name
		const i = query ? name.toLowerCase().indexOf(query.toLowerCase()) : -1
		if (i < 0) return { before: name, hit: '', after: '' }
		return {
			before: name.slice(0, i),
			hit: name.slice(i, i + query.length),
			after: name.slice(i + query.length),
		}
	})
</script>

<div class="group relative">
	<button
		type="button"
		class={cn(
			'flex w-full items-center gap-2.5 rounded-[9px] border-none py-[7px] pr-8 pl-2 text-left text-fg',
			'transition-[background-color,box-shadow] duration-150 ease-standard',
			selected
				? 'bg-hover shadow-[inset_0_0_0_1px_var(--line-2)]'
				: 'bg-transparent hover:bg-hover',
		)}
		aria-current={selected ? 'true' : undefined}
		aria-busy={connecting || undefined}
		title="{engine} · Double-click to connect · Right-click for actions"
		onclick={onselect}
		ondblclick={onopen}
		{oncontextmenu}
	>
		<ConnectionTile
			engine={connection.engine}
			color={colorHex(connection.colorId)}
			class="transition-transform duration-(--duration-spring) ease-spring group-hover:scale-[1.06] group-hover:-rotate-4"
		/>
		<span class="flex min-w-0 flex-1 flex-col gap-px">
			<span class="flex min-w-0 items-center gap-1.5">
				<span class="min-w-0 truncate font-medium">
					{nameParts.before}{#if nameParts.hit}<mark
							class="rounded-xs bg-accent-soft text-accent-text">{nameParts.hit}</mark
						>{/if}{nameParts.after}
				</span>
				{#if connection.environment === 'production'}
					<Pill tone="warning" uppercase compact>Prod</Pill>
				{/if}
				{#if connection.readOnly}
					<Pill tone="info" uppercase compact>RO</Pill>
				{/if}
			</span>
			<span class="truncate font-mono text-caption text-fg-muted">
				{connecting ? 'Connecting…' : `${engine} · ${target}`}
			</span>
		</span>
		{#if connecting}
			<StatusDot status="running" label="Connecting" />
		{:else if connection.status !== 'disconnected'}
			<StatusDot status={connection.status} />
		{/if}
	</button>

	<button
		type="button"
		class={cn(
			'absolute top-1/2 right-1.5 grid size-6 -translate-y-1/2 place-items-center rounded-md border-none bg-transparent',
			'transition-[opacity,color,background-color] duration-150 hover:bg-pressed focus-visible:opacity-100',
			connection.favorite
				? 'text-warning'
				: 'text-fg-muted opacity-0 group-hover:opacity-100 hover:text-fg',
		)}
		aria-label={connection.favorite ? 'Remove from favourites' : 'Add to favourites'}
		aria-pressed={connection.favorite}
		onclick={onfavorite}
		{oncontextmenu}
	>
		<Icon name={connection.favorite ? 'star-filled' : 'star'} class="size-3.5" />
	</button>
</div>
