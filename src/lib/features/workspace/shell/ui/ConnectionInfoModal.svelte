<script lang="ts">
	import { Button, Icon, IconButton, Modal, Pill } from '#core/ui'
	import { cn } from '#core/util'
	import { engineLabel } from '#engines'
	import {
		ConnectionTile,
		colorHex,
		connectionFacts,
		connectionUrlPreview,
		copyConnectionUrl,
	} from '#features/connections'
	import { liveStatusMark, workspaceSession } from '#features/workspace/session'
	import { connectionInfo } from '../connection-info.svelte'

	interface Props {
		/** Close the session (the rail's Disconnect). */
		ondisconnect?: (id: string) => void
	}

	let { ondisconnect }: Props = $props()

	const TITLE_ID = 'connection-info-title'
	const ENV = {
		production: { label: 'Prod', tone: 'warning' },
		staging: { label: 'Staging', tone: 'violet' },
		local: { label: 'Local', tone: 'neutral' },
	} as const

	const session = $derived(
		connectionInfo.id
			? workspaceSession.sessions.find((s) => s.profile.id === connectionInfo.id)
			: undefined,
	)
	const profile = $derived(session?.profile ?? null)
	const live = $derived(
		liveStatusMark(session?.liveStatus ?? profile?.status ?? 'disconnected'),
	)
	const env = $derived(profile ? (ENV[profile.environment] ?? ENV.local) : ENV.local)
	const urlPreview = $derived(profile ? connectionUrlPreview(profile) : '')
	const facts = $derived(profile ? connectionFacts(profile, 'summary') : [])
	const notes = $derived(profile?.notes?.trim() || '')

	let copying = $state(false)

	function close() {
		connectionInfo.id = null
	}

	async function copyUrl() {
		if (!profile || copying) return
		copying = true
		try {
			await copyConnectionUrl(profile)
		} finally {
			copying = false
		}
	}

	function disconnect() {
		const id = profile?.id
		close()
		if (id) ondisconnect?.(id)
	}
</script>

<Modal open={!!profile} onclose={close} layout="bare" size="sm" labelledby={TITLE_ID}>
	{#if profile}
		<div class="flex w-full flex-col">
			<header class="flex items-center gap-3 border-b border-line px-4 pt-4 pb-3.5">
				<ConnectionTile
					engine={profile.engine}
					color={colorHex(profile.colorId)}
					size="lg"
				/>
				<div class="flex min-w-0 flex-1 flex-col gap-0.5">
					<div class="flex min-w-0 items-center gap-2">
						<h2 id={TITLE_ID} class="m-0 min-w-0 truncate text-heading font-semibold">
							{profile.name}
						</h2>
						<Pill tone={env.tone} uppercase>{env.label}</Pill>
						{#if profile.readOnly}
							<Pill tone="info" uppercase>RO</Pill>
						{/if}
					</div>
					<p class="m-0 flex items-center gap-1.5 text-label text-fg-secondary">
						{engineLabel(profile.engine)} ·
						<span class={cn('inline-flex items-center gap-1 [&_svg]:size-3', live.class)} title={live.label}>
							<Icon name={live.icon} />{live.word}
						</span>
					</p>
				</div>
				<IconButton label="Close" variant="quiet" size="sm" onclick={close}>
					<Icon name="close" />
				</IconButton>
			</header>

			<div class="flex flex-col gap-3 px-4 py-3.5">
				<div
					class="flex items-center gap-2.5 rounded-[10px] bg-field py-2.5 pr-1.5 pl-3 shadow-[inset_0_0_0_1px_var(--line)]"
				>
					<div class="flex min-w-0 flex-1 flex-col gap-0.5">
						<span class="label-caps">Connection URL</span>
						<span class="truncate font-mono text-[0.71875rem] text-fg-secondary select-text" title={urlPreview}>
							{urlPreview || 'No URL'}
						</span>
					</div>
					<Button size="sm" disabled={copying} onclick={() => void copyUrl()}>
						{copying ? 'Copying…' : 'Copy'}
					</Button>
				</div>

				{#if facts.length > 0}
					<dl class="m-0 grid grid-cols-2 gap-x-4 gap-y-3">
						{#each facts as fact (fact.label)}
							<div class={cn('flex min-w-0 flex-col gap-0.5', fact.wide && 'col-span-2')}>
								<dt class="label-caps">{fact.label}</dt>
								<dd class="m-0 truncate text-[0.78125rem]" title={fact.value}>
									<span class={cn(fact.mono !== false && 'font-mono')}>{fact.value}</span>
									{#if fact.warn}
										<span class="text-[0.71875rem] text-warning">· {fact.warn}</span>
									{/if}
								</dd>
							</div>
						{/each}
					</dl>
				{/if}

				{#if notes}
					<div class="flex flex-col gap-0.5">
						<span class="label-caps">Notes</span>
						<p class="m-0 text-[0.78125rem] whitespace-pre-wrap text-fg-secondary select-text">{notes}</p>
					</div>
				{/if}
			</div>

			{#if ondisconnect}
				<footer class="flex justify-end gap-2 border-t border-line bg-surface px-3 py-2.5">
					<Button variant="danger-ghost" size="sm" onclick={disconnect}>Disconnect</Button>
				</footer>
			{/if}
		</div>
	{/if}
</Modal>
