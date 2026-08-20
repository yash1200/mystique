<script lang="ts">
	import { DefinitionList, Modal, Pill, StatusDot } from '$lib/components/core'
	import { EngineBadge } from '$lib/components/home'
	import type { ConnectionProfile } from '$lib/types/connection'
	import {
		colorHex,
		configStr,
		connectionEndpoint,
		engineLabel,
	} from '$lib/types/connection'

	interface Props {
		open: boolean
		profile: ConnectionProfile | null
		onclose: () => void
	}

	let { open, profile, onclose }: Props = $props()

	const hex = $derived(profile ? colorHex(profile.colorId) : '')
	const endpoint = $derived(profile ? connectionEndpoint(profile) : '')

	const envLabel = $derived(
		profile?.environment === 'production'
			? 'Production'
			: profile?.environment === 'staging'
				? 'Staging'
				: 'Local',
	)

	const infoItems = $derived.by(() => {
		if (!profile) return []
		const items: {
			label: string
			value: string
			mono?: boolean
			full?: boolean
		}[] = [
			{ label: 'Engine', value: engineLabel(profile.engine) },
			{ label: 'Endpoint', value: endpoint, mono: true, full: true },
			{ label: 'Environment', value: envLabel },
			{
				label: 'Access',
				value: profile.readOnly ? 'Read-only' : 'Read / write',
			},
		]
		const database = configStr(profile.pluginConfig ?? {}, 'database')
		const username = configStr(profile.pluginConfig ?? {}, 'username')
		if (database) {
			items.push({ label: 'Database', value: database, mono: true })
		}
		if (username) {
			items.push({ label: 'Username', value: username, mono: true })
		}
		if (profile.notes?.trim()) {
			items.push({
				label: 'Notes',
				value: profile.notes.trim(),
				full: true,
			})
		}
		return items
	})

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
	{onclose}
	layout="chrome"
	size="md"
	labelledby="connection-info-title"
	style="--connection-color: {hex}"
>
	{#snippet header()}
		{#if profile}
			<div class="flex flex-wrap items-center gap-1.5">
				<h2
					id="connection-info-title"
					class="m-0 truncate text-body font-semibold text-fg"
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
				<span class="text-fg-muted" aria-hidden="true">·</span>
				<StatusDot status="connected" />
				<span class="text-success">Connected</span>
			</p>
		{/if}
	{/snippet}

	<div class="px-4 py-4">
		<DefinitionList items={infoItems} />
		<p class="mt-4 mb-0 text-caption text-fg-muted">
			Last connected · {formatDate(profile?.lastConnectedAt)}
		</p>
	</div>
</Modal>
