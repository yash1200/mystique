<script lang="ts">
	import { Button, Field, Input, Modal } from '#core/ui'
	import {
		currentWriteGuard,
		resolveWriteGuard,
		subscribeWriteGuard,
	} from '../write-guard'
	import { onMount } from 'svelte'

	let prompt = $state(currentWriteGuard())
	let typed = $state('')
	onMount(() =>
		subscribeWriteGuard(() => {
			prompt = currentWriteGuard()
			typed = ''
		}),
	)

	const expected = $derived(prompt?.objectName?.trim() ?? '')
	const typeOk = $derived(
		prompt?.kind !== 'typeConfirm' ||
			typed.trim().toLowerCase() === expected.toLowerCase(),
	)
	const envLabel = $derived(
		(prompt?.environment ?? 'production').toLowerCase() === 'staging'
			? 'Staging'
			: 'Production',
	)

	function cancel() {
		resolveWriteGuard({ ok: false })
	}

	function confirm() {
		if (!prompt) return
		if (prompt.kind === 'typeConfirm') {
			if (!typeOk) return
			resolveWriteGuard({ ok: true, typedName: typed.trim() })
			return
		}
		resolveWriteGuard({ ok: true })
	}
</script>

<Modal
	open={!!prompt}
	onclose={cancel}
	title={prompt?.kind === 'typeConfirm'
		? `Confirm ${envLabel} write`
		: `Allow writes on ${envLabel}`}
	description={prompt
		? prompt.kind === 'typeConfirm'
			? `“${prompt.connectionName}” is ${envLabel.toLowerCase()}. Type “${expected}” to continue.`
			: `“${prompt.connectionName}” is ${envLabel.toLowerCase()}. The first write this session needs confirmation. Unlock lasts until you disconnect or quit.`
		: ''}
	dismissible
>
	{#if prompt?.kind === 'typeConfirm'}
		<Field label="Object name" full>
			<Input bind:value={typed} autocomplete="off" />
		</Field>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={cancel}>Cancel</Button>
		<Button
			variant="danger"
			size="sm"
			disabled={prompt?.kind === 'typeConfirm' && !typeOk}
			onclick={confirm}
		>
			{prompt?.kind === 'typeConfirm' ? 'Run' : 'Allow writes this session'}
		</Button>
	{/snippet}
</Modal>
