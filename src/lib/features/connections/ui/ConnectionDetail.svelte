<script lang="ts">
	import type { ConnectionProfile } from '#core/contracts'
	import { Button, ContentPanel, Pill, StatusDot } from '#core/ui'
	import { cn } from '#core/util'
	import { engineLabel } from '#engines'
	import { hasProduction, productionLabel } from '../environments'
	import { connectionFacts } from '../facts'
	import { colorHex, connectionEndpoint } from '../model'
	import ConnectionFactGrid from './ConnectionFactGrid.svelte'
	import EngineBadge from './EngineBadge.svelte'
	import ProductionMark from './ProductionMark.svelte'

	interface Props {
		connection: ConnectionProfile
		testing?: boolean
		onedit?: () => void
		onconnect?: () => void
		ontest?: () => void | Promise<void>
		onduplicate?: () => void
		ondelete?: () => void
	}

	let {
		connection,
		testing = false,
		onedit,
		onconnect,
		ontest,
		onduplicate,
		ondelete,
	}: Props = $props()

	const hex = $derived(colorHex(connection.colorId))
	const status = $derived(connection.status ?? 'disconnected')
	const endpoint = $derived(connectionEndpoint(connection))

	const statusText = $derived(
		status === 'connected'
			? 'Connected'
			: status === 'error'
				? 'Last attempt failed'
				: 'Disconnected',
	)

	const facts = $derived(connectionFacts(connection, 'full'))

	const notes = $derived(connection.notes?.trim() || '')
</script>

<ContentPanel size="lg" center ariaLabel="Connection details">
	<article
		style="--connection-color: {hex}"
		class="overflow-hidden rounded-xl border border-line bg-elevated shadow-md"
	>
		<div class="grid md:grid-cols-[15.5rem_minmax(0,1fr)]">
			<!-- Identity -->
			<div
				class="flex flex-col justify-between gap-6 border-b border-line conn-tint-soft px-5 py-6 md:border-r md:border-b-0"
			>
				<div>
					<div
						class="mb-4 size-10 rounded-lg border border-line bg-conn shadow-sm"
						aria-hidden="true"
					></div>
					<h1
						class="m-0 text-pretty text-title font-semibold tracking-tight text-fg"
					>
						{connection.name}
					</h1>
					<div class="mt-2 flex flex-wrap items-center gap-1.5">
						<EngineBadge engine={connection.engine} size="md" />
						{#if hasProduction(connection)}
							<ProductionMark label={productionLabel(connection)} />
						{/if}
						{#if connection.readOnly}
							<Pill uppercase>RO</Pill>
						{/if}
					</div>
					<p class="mt-3 text-label text-fg-secondary">
						{engineLabel(connection.engine)}
					</p>
					<p class="mt-3 flex items-center gap-2 text-label">
						<StatusDot {status} />
						<span
							class={cn(
								status === 'connected' && 'text-success',
								status === 'error' && 'text-danger',
								status === 'disconnected' && 'text-fg-muted',
							)}
						>
							{statusText}
						</span>
					</p>
				</div>
				<Button
					variant="primary"
					fullWidth
					onclick={onconnect}
					disabled={testing}
				>
					{#if testing}
						Connecting…
					{:else if status === 'connected'}
						Reconnect
					{:else}
						Connect
					{/if}
				</Button>
			</div>

			<!-- Facts -->
			<div class="flex min-w-0 flex-col px-5 py-5">
				<p
					class="m-0 mb-1.5 text-micro font-semibold tracking-wider text-fg-muted uppercase"
				>
					Target
				</p>
				<p
					class="m-0 mb-4 font-mono text-body leading-relaxed break-all text-fg"
				>
					{endpoint}
				</p>

				<ConnectionFactGrid {facts} stack />

				{#if notes}
					<p class="mt-4 mb-0 text-body leading-relaxed text-fg-secondary">
						{notes}
					</p>
				{/if}

				<div class="mt-auto flex flex-wrap gap-2 pt-5">
					<Button variant="ghost" size="sm" onclick={onedit} disabled={testing}
						>Edit</Button
					>
					<Button
						variant="ghost"
						size="sm"
						onclick={onduplicate}
						disabled={testing}>Duplicate</Button
					>
					{#if ontest}
						<Button
							variant="secondary"
							size="sm"
							onclick={ontest}
							disabled={testing}
						>
							{testing ? 'Testing…' : 'Test'}
						</Button>
					{/if}
					<Button
						variant="danger-ghost"
						size="sm"
						disabled={testing}
						onclick={() => ondelete?.()}
					>
						Delete
					</Button>
				</div>
			</div>
		</div>
	</article>
</ContentPanel>
