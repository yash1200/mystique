<script lang="ts">
	import { Button, Field, Icon, Modal, PasswordInput } from '#core/ui'
	import { passwordPrompt } from '../password-prompt.svelte'

	const TITLE_ID = 'password-prompt-title'

	let password = $state('')

	const target = $derived(passwordPrompt.target)

	function submit(e: SubmitEvent) {
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

<Modal open={!!target} onclose={cancel} labelledby={TITLE_ID} size="sm">
	<form id="password-prompt" class="m-0 flex flex-col gap-4" onsubmit={submit}>
		<header class="flex items-center gap-3">
			<span
				class="grid size-8 shrink-0 place-items-center rounded-[9px] bg-hover text-fg-secondary [&_svg]:size-4"
			>
				<Icon name="lock" />
			</span>
			<div class="min-w-0">
				<h2 id={TITLE_ID} class="m-0 text-heading font-semibold tracking-tight">
					Enter password
				</h2>
				<p class="m-0 truncate text-label text-fg-muted">{target?.name}</p>
			</div>
		</header>
		<Field label="Password" hint="Kept in memory until you quit Mystique. Not saved.">
			<PasswordInput
				bind:value={password}
				autocomplete="current-password"
				autofocus
			/>
		</Field>
	</form>
	{#snippet footer()}
		<Button variant="ghost" onclick={cancel}>Cancel</Button>
		<Button type="submit" form="password-prompt" variant="primary" disabled={!password}>
			Connect
		</Button>
	{/snippet}
</Modal>
