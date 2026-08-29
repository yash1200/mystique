<script lang="ts">
	import type { ParsedConnectionUrl } from '$core/contracts'
	import {
		Card,
		Checkbox,
		Field,
		FormGrid,
		Input,
		PasswordInput,
	} from '$core/ui'
	import type { EngineFieldsProps } from '$engines'
	import ImportConnectionUrl from '../ui/ImportConnectionUrl.svelte'

	let {
		draft = $bindable(),
		isEdit = false,
		passwordSet = false,
		disabled = false,
	}: EngineFieldsProps = $props()

	function applyParsed(parsed: ParsedConnectionUrl) {
		const cfg = parsed.pluginConfig ?? {}
		const h = cfg.host
		if (typeof h === 'string' && h.trim()) draft.host = h.trim()
		const p = cfg.port
		if (typeof p === 'number' && Number.isFinite(p)) draft.port = p
		else if (typeof p === 'string' && p.trim()) {
			const n = Number(p)
			if (Number.isFinite(n)) draft.port = n
		}
		const db = cfg.database
		if (typeof db === 'string') draft.database = db
		const user = cfg.username ?? cfg.user
		if (typeof user === 'string') draft.username = user
		if (typeof cfg.ssl === 'boolean') draft.ssl = cfg.ssl
		else if (cfg.sslMode === 'require') draft.ssl = true
		if (typeof parsed.password === 'string') draft.password = parsed.password
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
			<Input bind:value={draft.host} autocomplete="off" {disabled} />
		</Field>
		<Field label="Port">
			<Input
				type="number"
				bind:value={draft.port}
				min="1"
				max="65535"
				{disabled}
			/>
		</Field>
		<Field label="Database" full>
			<Input bind:value={draft.database} autocomplete="off" {disabled} />
		</Field>
		<Field label="Username">
			<Input
				bind:value={draft.username}
				autocomplete="username"
				placeholder="postgres"
				{disabled}
			/>
		</Field>
		<Field label="Password">
			<PasswordInput
				bind:value={draft.password}
				autocomplete="new-password"
				placeholder={isEdit && passwordSet ? '••••••••' : 'Optional'}
				{disabled}
			/>
		</Field>
	</FormGrid>

	<div class="mt-3 flex flex-wrap gap-4">
		<Checkbox bind:checked={draft.ssl} label="Require SSL" {disabled} />
		<Checkbox bind:checked={draft.readOnly} label="Read-only" {disabled} />
	</div>
</Card>
