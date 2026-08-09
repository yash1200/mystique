<script lang="ts">
	import {
		Card,
		Checkbox,
		Field,
		FormGrid,
		Input,
		PasswordInput,
	} from '$lib/components/ui'

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
	}

	let {
		host = $bindable(''),
		port = $bindable(3306),
		database = $bindable('mystique'),
		username = $bindable('admin'),
		password = $bindable(''),
		ssl = $bindable(false),
		readOnly = $bindable(false),
		isEdit = false,
		passwordSet = false,
	}: Props = $props()
</script>

<Card title="Server">
	<FormGrid>
		<Field label="Host">
			<Input bind:value={host} autocomplete="off" />
		</Field>
		<Field label="Port">
			<Input type="number" bind:value={port} min="1" max="65535" />
		</Field>
		<Field label="Database" full>
			<Input bind:value={database} autocomplete="off" placeholder="mystique" />
		</Field>
		<Field label="Username">
			<Input
				bind:value={username}
				autocomplete="username"
				placeholder="admin"
			/>
		</Field>
		<Field label="Password">
			<PasswordInput
				bind:value={password}
				autocomplete="new-password"
				placeholder={isEdit && passwordSet ? '••••••••' : '123'}
			/>
		</Field>
	</FormGrid>

	<div class="mt-3 flex flex-wrap gap-4">
		<Checkbox bind:checked={ssl} label="Require SSL" />
		<Checkbox bind:checked={readOnly} label="Read-only" />
	</div>
</Card>
