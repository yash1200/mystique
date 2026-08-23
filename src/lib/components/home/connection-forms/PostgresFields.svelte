<script lang="ts">
	import {
		Card,
		Checkbox,
		Field,
		FormGrid,
		Input,
		PasswordInput,
	} from '$lib/components/core'
	import type { ParsedConnectionUrl } from '$lib/api/engines'
	import ImportConnectionUrl from './ImportConnectionUrl.svelte'

	interface Props {
		host?: string
		port?: number
		database?: string
		username?: string
		password?: string
		ssl?: boolean
		readOnly?: boolean
		/** Edit mode: show “leave blank to keep password” UX. */
		isEdit?: boolean
		passwordSet?: boolean
		disabled?: boolean
	}

	let {
		host = $bindable(''),
		port = $bindable(5432),
		database = $bindable(''),
		username = $bindable(''),
		password = $bindable(''),
		ssl = $bindable(false),
		readOnly = $bindable(false),
		isEdit = false,
		passwordSet = false,
		disabled = false,
	}: Props = $props()

	function applyParsed(parsed: ParsedConnectionUrl) {
		const cfg = parsed.pluginConfig ?? {}
		const h = cfg.host
		if (typeof h === 'string' && h.trim()) host = h.trim()
		const p = cfg.port
		if (typeof p === 'number' && Number.isFinite(p)) port = p
		else if (typeof p === 'string' && p.trim()) {
			const n = Number(p)
			if (Number.isFinite(n)) port = n
		}
		const db = cfg.database
		if (typeof db === 'string') database = db
		const user = cfg.username ?? cfg.user
		if (typeof user === 'string') username = user
		if (typeof cfg.ssl === 'boolean') ssl = cfg.ssl
		else if (cfg.sslMode === 'require') ssl = true
		if (typeof parsed.password === 'string') password = parsed.password
	}
</script>

<Card title="Server">
	<ImportConnectionUrl
		engine="postgres"
		placeholder="postgresql://user:pass@host:5432/dbname?sslmode=require"
		{disabled}
		onparsed={applyParsed}
	/>
	<FormGrid>
		<Field label="Host">
			<Input bind:value={host} autocomplete="off" {disabled} />
		</Field>
		<Field label="Port">
			<Input type="number" bind:value={port} min="1" max="65535" {disabled} />
		</Field>
		<Field label="Database" full>
			<Input bind:value={database} autocomplete="off" {disabled} />
		</Field>
		<Field label="Username">
			<Input
				bind:value={username}
				autocomplete="username"
				placeholder="postgres"
				{disabled}
			/>
		</Field>
		<Field label="Password">
			<PasswordInput
				bind:value={password}
				autocomplete="new-password"
				placeholder={isEdit && passwordSet ? '••••••••' : 'Optional'}
				{disabled}
			/>
		</Field>
	</FormGrid>

	<div class="mt-3 flex flex-wrap gap-4">
		<Checkbox bind:checked={ssl} label="Require SSL" {disabled} />
		<Checkbox bind:checked={readOnly} label="Read-only" {disabled} />
	</div>
</Card>
