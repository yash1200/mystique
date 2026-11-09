<script lang="ts">
	import { Icon, IconButton, Modal, Pill } from '#core/ui'
	import { cn } from '#core/util'
	import { engineLabel } from '#engines'
	import {
		ConnectionFactGrid,
		EngineBadge,
		colorHex,
		connectionFacts,
		connectionInitial,
		connectionUrlPreview,
		copyConnectionUrl,
		environmentLabel,
	} from '#features/connections'
	import { liveStatusMark, workspaceSession } from '#features/workspace/session'
	import { connectionInfo } from '../connection-info.svelte'

	const session = $derived(
		connectionInfo.id
			? workspaceSession.sessions.find((s) => s.profile.id === connectionInfo.id)
			: undefined,
	)
	const profile = $derived(session?.profile ?? null)

	let copyBusy = $state(false)

	function handleClose() {
		connectionInfo.id = null
	}

	async function copyUrl() {
		if (!profile || copyBusy) return
		copyBusy = true
		try {
			await copyConnectionUrl(profile)
		} finally {
			copyBusy = false
		}
	}

	const hex = $derived(profile ? colorHex(profile.colorId) : '')
	const initial = $derived(
		profile ? connectionInitial(profile.name, String(profile.engine)) : '',
	)
	const liveMark = $derived(
		liveStatusMark(session?.liveStatus ?? profile?.status ?? 'disconnected'),
	)
	const urlPreview = $derived(profile ? connectionUrlPreview(profile) : '')
	const facts = $derived(profile ? connectionFacts(profile, 'summary') : [])
	const notes = $derived(profile?.notes?.trim() || '')
</script>

<Modal
	open={!!profile}
	onclose={handleClose}
	layout="bare"
	size="md"
	labelledby="connection-info-title"
	class="overflow-hidden"
	style="--connection-color: {hex}"
>
	{#if profile}
		<span class="absolute inset-x-0 top-0 h-0.5 bg-conn" aria-hidden="true"
		></span>

		<header
			class="flex items-start gap-3 border-b border-line conn-tint-soft px-5 pt-5 pb-4"
		>
			<span
				class="grid size-10 shrink-0 place-items-center rounded-lg border border-line bg-(--connection-color)/15 text-body-lg font-semibold"
				style="color: var(--connection-color)"
				aria-hidden="true"
			>
				{initial}
			</span>
			<div class="min-w-0 flex-1">
				<div class="flex flex-wrap items-center gap-1.5">
					<h2
						id="connection-info-title"
						class="m-0 min-w-0 truncate text-title font-semibold tracking-tight text-fg"
					>
						{profile.name}
					</h2>
					{#if profile.environment === 'production'}
						<Pill tone="warning" uppercase>PROD</Pill>
					{/if}
					{#if profile.readOnly}
						<Pill uppercase>RO</Pill>
					{/if}
				</div>
				<p
					class="mt-1.5 mb-0 flex flex-wrap items-center gap-1.5 text-label text-fg-secondary"
				>
					<EngineBadge engine={profile.engine} />
					<span>{engineLabel(profile.engine)}</span>
					{#if profile.environment === 'staging'}
						<span class="text-fg-muted">{environmentLabel(profile.environment)}</span>
					{/if}
					<span
						class="inline-flex items-center gap-1 {liveMark.class}"
						title={liveMark.label}
					>
						<Icon name={liveMark.icon} class="size-3.5" />
						<span>{liveMark.word}</span>
					</span>
				</p>
			</div>
			<IconButton label="Close" variant="ghost" size="sm" onclick={handleClose}>
				<Icon name="close" />
			</IconButton>
		</header>

		<div class="flex flex-col gap-4 px-5 py-4">
			<div>
				<button
					type="button"
					class={cn(
						'flex w-full items-center gap-3 rounded-lg border bg-input px-3 py-2.5 text-left',
						'transition-[color,background-color,border-color,transform] duration-150 ease-out',
						'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus',
						'enabled:active:scale-[0.99]',
						'border-line hover:border-line-strong hover:bg-hover',
					)}
					disabled={copyBusy || !profile}
					aria-label="Copy connection URL"
					onclick={() => void copyUrl()}
				>
					<div class="min-w-0 flex-1">
						<p class="m-0 text-label font-medium text-fg">
							{copyBusy ? 'Copying…' : 'Copy connection URL'}
						</p>
						<p
							class="mt-0.5 mb-0 truncate font-mono text-caption text-fg-muted"
							title={urlPreview}
						>
							{urlPreview || 'No URL'}
						</p>
					</div>
					<Icon name="copy" class="text-fg-secondary" />
				</button>
			</div>

			{#if facts.length > 0}
				<ConnectionFactGrid {facts} />
			{/if}

			{#if notes}
				<div>
					<p class="m-0 text-label font-medium text-fg-secondary">Notes</p>
					<p
						class="mt-1 mb-0 text-body leading-relaxed whitespace-pre-wrap text-fg"
					>
						{notes}
					</p>
				</div>
			{/if}
		</div>
	{/if}
</Modal>
