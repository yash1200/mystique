<script lang="ts">
	import type { ConnectionProfile } from '#core/contracts'
	import { errorMessage, formatEngineConnectionUrl, isTauri } from '#core/ipc'
	import { Icon, IconButton, Modal, Pill } from '#core/ui'
	import { cn, copyTextToClipboard } from '#core/util'
	import {
		connectionKindFromCaps,
		databaseFromUri,
		defaultCapabilitiesForEngine,
		engineLabel,
	} from '#engines'
	import {
		EngineBadge,
		colorHex,
		configBool,
		configNum,
		configStr,
		connectionEndpoint,
		connectionInitial,
	} from '#features/connections'
	import { showToast } from '#features/toast'
	import { liveStatusMark, workspaceSession } from '#features/workspace/session'

	interface Props {
		open: boolean
		profile: ConnectionProfile | null
		onclose: () => void
	}

	let { open, profile, onclose }: Props = $props()

	let copyBusy = $state(false)

	function handleClose() {
		onclose()
	}

	async function copyConnectionUrl() {
		if (!profile || copyBusy) return
		if (!isTauri()) {
			showToast(
				'warning',
				'Copy connection URL needs the Mystique desktop app.',
			)
			return
		}
		copyBusy = true
		try {
			const formatted = await formatEngineConnectionUrl({
				engine: String(profile.engine),
				pluginConfig: (profile.pluginConfig ?? {}) as Record<string, unknown>,
			})
			const url = formatted.url?.trim()
			if (!url) {
				showToast('warning', 'No connection URL available for this profile.')
				return
			}
			const result = await copyTextToClipboard(url)
			if (result.ok) {
				showToast('success', 'Connection URL copied.')
			} else if (!result.cancelled) {
				showToast('danger', result.error || 'Could not copy connection URL.')
			}
		} catch (err) {
			showToast('danger', errorMessage(err))
		} finally {
			copyBusy = false
		}
	}

	const hex = $derived(profile ? colorHex(profile.colorId) : '')
	const initial = $derived(
		profile ? connectionInitial(profile.name, String(profile.engine)) : '',
	)
	const endpoint = $derived(profile ? connectionEndpoint(profile) : '')
	const liveStatus = $derived(
		profile
			? (workspaceSession.sessions.find((s) => s.profile.id === profile.id)
					?.liveStatus ??
					profile.status ??
					'disconnected')
			: undefined,
	)
	const liveMark = $derived(liveStatusMark(liveStatus))
	const kind = $derived(
		profile
			? connectionKindFromCaps(defaultCapabilitiesForEngine(profile.engine))
			: 'host',
	)

	const envLabel = $derived(
		profile?.environment === 'production'
			? 'Production'
			: profile?.environment === 'staging'
				? 'Staging'
				: 'Local',
	)

	const database = $derived.by(() => {
		if (!profile) return ''
		const cfg = profile.pluginConfig ?? {}
		return (
			configStr(cfg, 'database') ||
			databaseFromUri(profile.engine, configStr(cfg, 'uri')) ||
			''
		)
	})

	const username = $derived.by(() => {
		if (!profile) return ''
		return configStr(profile.pluginConfig ?? {}, 'username')
	})

	const sslLabel = $derived.by(() => {
		if (!profile || kind !== 'host') return ''
		const cfg = profile.pluginConfig ?? {}
		if (configBool(cfg, 'ssl') || configStr(cfg, 'sslMode') === 'require') {
			return 'Required'
		}
		return 'Off'
	})

	const urlPreview = $derived.by(() => {
		if (!profile) return ''
		const cfg = profile.pluginConfig ?? {}
		if (kind === 'file') return configStr(cfg, 'path') || endpoint
		if (kind === 'uri') return redactUrl(configStr(cfg, 'uri') || endpoint)
		const host = configStr(cfg, 'host')
		const port = configNum(cfg, 'port', 0)
		const user = configStr(cfg, 'username')
		const db = configStr(cfg, 'database')
		const hostPort = host ? (port ? `${host}:${port}` : host) : ''
		if (!hostPort) return endpoint
		return `${user ? `${user}@` : ''}${hostPort}${db ? `/${db}` : ''}`
	})

	const facts = $derived.by(() => {
		if (!profile) return []
		const items: { label: string; value: string; wide?: boolean }[] = []
		if (database) items.push({ label: 'Database', value: database })
		if (username) items.push({ label: 'User', value: username })
		if (sslLabel) items.push({ label: 'SSL', value: sslLabel })
		items.push({
			label: 'Access',
			value: profile.readOnly ? 'Read-only' : 'Read / write',
		})
		items.push({
			label: 'Last connected',
			value: formatDate(profile.lastConnectedAt),
			wide: items.length % 2 === 0,
		})
		return items
	})

	const notes = $derived(profile?.notes?.trim() || '')

	function redactUrl(uri: string): string {
		try {
			const parsed = new URL(uri)
			if (parsed.password) parsed.password = '••••'
			return parsed.toString()
		} catch {
			return uri.replace(/:([^:@/]+)@/, ':••••@')
		}
	}

	function formatDate(iso: string | null | undefined): string {
		if (!iso) return 'Never'
		try {
			return new Intl.DateTimeFormat(undefined, {
				dateStyle: 'medium',
				timeStyle: 'short',
			}).format(new Date(iso))
		} catch {
			return iso
		}
	}

</script>

<Modal
	open={open && !!profile}
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
						<span class="text-fg-muted">{envLabel}</span>
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
					onclick={() => void copyConnectionUrl()}
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
				<div
					class="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line"
				>
					{#each facts as item (item.label)}
						<div
							class={cn('bg-elevated px-3 py-2.5', item.wide && 'col-span-2')}
						>
							<div
								class="text-micro font-semibold tracking-wider text-fg-muted uppercase"
							>
								{item.label}
							</div>
							<div
								class="mt-1 min-w-0 truncate font-mono text-label text-fg"
								title={item.value}
							>
								{item.value}
							</div>
						</div>
					{/each}
				</div>
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
