<script lang="ts">
	import { Button, Field, Modal, PasswordInput } from '#core/ui'
	import { passwordPrompt } from '../password-prompt.svelte'

	let password = $state('')

	const target = $derived(passwordPrompt.target)

	function submit(e: Event) {
		e.preventDefault()
		if (!password) return
		const typed = password
		password = ''
		passwordPrompt.submit(typed)
	}

	function cancel() {
		password = ''
		passwordPrompt.cancel()
	}
</script>

<Modal
	open={!!target}
	onclose={cancel}
	title="Enter password"
	description={target?.name}
	layout="inset"
	size="sm"
>
	<form id="password-prompt" class="m-0" onsubmit={submit}>
		<Field label="Password">
			<PasswordInput
				bind:value={password}
				autocomplete="current-password"
				autofocus
			/>
		</Field>
		<p class="m-0 mt-2 text-caption text-fg-muted">
			Kept in memory until you quit Mystique. Not saved.
		</p>
	</form>
	{#snippet footer()}
		<Button variant="ghost" onclick={cancel}>Cancel</Button>
		<Button
			type="submit"
			form="password-prompt"
			variant="primary"
			disabled={!password}
		>
			Connect
		</Button>
	{/snippet}
</Modal>
