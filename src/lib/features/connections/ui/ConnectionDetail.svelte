<script lang="ts">
	import { Alert, Button, Icon, Pill, Spinner, StatusDot } from '#core/ui'
	import { cn, copyTextToClipboard } from '#core/util'
	import { showToast } from '#features/toast'
	import { engineLabel } from '#engines'
	import { connectionFacts, connectionTarget } from '../facts'
	import { getHome, type HomeConnection } from '../home.svelte'
	import { colorHex } from '../model'
	import ConnectionFactGrid from './ConnectionFactGrid.svelte'
	import ConnectionTile from './ConnectionTile.svelte'

	interface Props {
		connection: HomeConnection
	}

	let { connection }: Props = $props()

	const home = getHome()

	const ENV = {
		production: { label: 'Prod', tone: 'warning' },
		staging: { label: 'Staging', tone: 'violet' },
		local: { label: 'Local', tone: 'neutral' },
	} as const

	let copied = $state(false)
	let copiedTimer: ReturnType<typeof setTimeout> | undefined

	const env = $derived(ENV[connection.environment] ?? ENV.local)
	const isProd = $derived(connection.environment === 'production')
	const target = $derived(connectionTarget(connection))
	const facts = $derived(connectionFacts(connection, 'full'))
	const notes = $derived(connection.notes?.trim() ?? '')

	const busy = $derived(home.busy)
	const connecting = $derived(busy?.kind === 'connect' && busy.id === connection.id)
	const testing = $derived(busy?.kind === 'test' && busy.id === connection.id)
	const health = $derived(home.health?.id === connection.id ? home.health : null)

	/** The host's first label, for "Reaching prod-cluster…". */
	const shortHost = $derived(target.replace(/^[a-z+]+:\/\//i, '').split(/[.:/]/)[0] || target)

	async function copyTarget() {
		const result = await copyTextToClipboard(target)
		if (!result.ok) {
			if (!result.cancelled) showToast('danger', result.error || 'Could not copy the target.')
			return
		}
		copied = true
		clearTimeout(copiedTimer)
		copiedTimer = setTimeout(() => (copied = false), 1500)
	}

	$effect(() => () => clearTimeout(copiedTimer))
</script>

<section
	class="stagger mx-auto flex w-full max-w-panel-xl flex-col gap-5 px-10 pt-10 pb-36"
	aria-label="Connection details"
>
	<header class="flex flex-wrap items-start gap-5">
		<ConnectionTile engine={connection.engine} color={colorHex(connection.colorId)} size="xl" />
		<div class="flex min-w-0 flex-[1_1_18rem] flex-col gap-2">
			<div class="flex flex-wrap items-center gap-2.5">
				<h1 class="m-0 min-w-0 text-display font-semibold tracking-[-0.025em] break-words">
					{connection.name}
				</h1>
				<Pill tone={env.tone} uppercase>{env.label}</Pill>
				{#if connection.readOnly}
					<Pill tone="info" uppercase>
						<Icon name="lock" />Read-only
					</Pill>
				{/if}
			</div>
			<p class="m-0 flex flex-wrap items-center gap-2 text-fg-secondary">
				<span>{engineLabel(connection.engine)}</span>
				<span class="text-fg-muted" aria-hidden="true">·</span>
				{#if connecting}
					<span class="inline-flex items-center gap-1.5">
						<Spinner size="xs" />Connecting…
					</span>
				{:else if connection.status === 'connected'}
					<span class="inline-flex items-center gap-2 text-accent-text">
						<StatusDot status="connected" />Connected
					</span>
				{:else if connection.status === 'error'}
					<span class="inline-flex items-center gap-2 text-danger">
						<StatusDot status="error" />Last attempt failed
					</span>
				{:else}
					<span class="inline-flex items-center gap-2">
						<StatusDot status="disconnected" />Disconnected
					</span>
				{/if}
			</p>
		</div>
		<div class="flex items-center gap-1.5">
			<Button variant="ghost" disabled={!!busy} onclick={() => home.edit(connection.id)}>
				<Icon name="pencil" />Edit
			</Button>
			<Button
				variant="ghost"
				disabled={!!busy || home.saving}
				onclick={() => void home.duplicate(connection.id)}
			>
				<Icon name="copy" />Duplicate
			</Button>
			<Button
				variant="primary"
				size="lg"
				class="min-w-[9.5rem]"
				disabled={!!busy}
				aria-busy={connecting}
				onclick={() => void home.connect(connection.id)}
			>
				{#if connecting}
					<Spinner />Connecting…
				{:else if connection.status === 'connected'}
					Reconnect
				{:else}
					Connect
				{/if}
			</Button>
		</div>
	</header>

	{#if isProd}
		<Alert tone="warning" banner>
			Production with <strong class="font-semibold">{connection.readOnly ? 'read-only' : 'read / write'}</strong>
			access. Double-check the target before running anything that changes data.
		</Alert>
	{/if}

	<div class="flex items-center gap-4 rounded-lg border border-line bg-surface py-4 pr-4 pl-5">
		<div class="flex min-w-0 flex-1 flex-col gap-1">
			<span class="label-caps">Target</span>
			<span class="truncate font-mono text-[0.84375rem] select-text" title={target}>{target}</span>
		</div>
		<Button size="sm" aria-label="Copy target" onclick={copyTarget}>
			{#if copied}
				<Icon name="check" class="animate-rise text-accent-text" />Copied
			{:else}
				<Icon name="copy" />Copy
			{/if}
		</Button>
	</div>

	<ConnectionFactGrid {facts} layout="fluid" />

	<div class="grid grid-cols-[repeat(auto-fit,minmax(min(20rem,100%),1fr))] gap-5">
		<section
			class="flex flex-col gap-3.5 rounded-lg border border-line bg-surface px-5 py-[1.125rem]"
			aria-labelledby="health-title"
		>
			<div class="flex items-center justify-between gap-3">
				<h2 id="health-title" class="m-0 text-body-lg font-semibold">Health check</h2>
				<Button size="sm" disabled={!!busy} onclick={() => void home.testSaved(connection.id)}>
					{#if testing}
						<Spinner size="sm" />
					{:else}
						<Icon name="bolt" />
					{/if}
					Test connection
				</Button>
			</div>
			<p class="m-0" aria-live="polite">
				{#if testing}
					<span class="animate-rise text-fg-secondary">Reaching {shortHost}…</span>
				{:else if health?.ok}
					<span class="inline-flex animate-rise flex-wrap items-center gap-2 text-accent-text">
						<Icon name="check" />Reachable · auth ok · {health.latencyMs} ms
						{#if health.version}
							<span class="font-mono text-label text-fg-muted">{health.version}</span>
						{/if}
					</span>
				{:else if health}
					<span class="flex animate-rise items-start gap-2 text-danger">
						<Icon name="alert-circle" class="mt-0.5" />
						<span class="min-w-0 break-words select-text">{health.message}</span>
					</span>
				{:else}
					<span class="text-fg-muted">Not tested yet.</span>
				{/if}
			</p>
		</section>

		<section
			class="flex flex-col gap-3.5 rounded-lg border border-line bg-surface px-5 py-[1.125rem]"
			aria-labelledby="notes-title"
		>
			<div class="flex items-center justify-between gap-3">
				<h2 id="notes-title" class="m-0 text-body-lg font-semibold">Notes</h2>
				<Button
					variant="ghost"
					size="sm"
					disabled={!!busy}
					onclick={() => home.edit(connection.id)}
				>
					<Icon name={notes ? 'pencil' : 'plus'} />{notes ? 'Edit note' : 'Add note'}
				</Button>
			</div>
			<p
				class={cn(
					'm-0 whitespace-pre-line',
					notes ? 'text-fg-secondary select-text' : 'text-fg-muted',
				)}
			>
				{notes || 'No notes yet. Leave context for your team, like who owns this database.'}
			</p>
		</section>
	</div>

	<div class="flex justify-end pt-1">
		<Button
			variant="danger-ghost"
			size="sm"
			disabled={!!busy}
			onclick={() => home.requestDelete(connection.id)}
		>
			<Icon name="trash" />Delete connection
		</Button>
	</div>
</section>
