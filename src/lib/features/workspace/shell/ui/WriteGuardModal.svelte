<script lang="ts">
	import { Button, Field, Input, Modal } from '#core/ui'
	import {
		guardCopy,
		typedNameOk,
		currentWriteGuard,
		resolveWriteGuard,
		subscribeWriteGuard,
	} from '#features/workspace/session'
	import { onMount } from 'svelte'

	let prompt = $state(currentWriteGuard())
	let typed = $state('')
	onMount(() =>
		subscribeWriteGuard(() => {
			prompt = currentWriteGuard()
			typed = ''
		}),
	)

	const copy = $derived(prompt ? guardCopy(prompt) : null)
	const typeOk = $derived(!prompt || typedNameOk(prompt, typed))

	function cancel() {
		resolveWriteGuard({ ok: false })
	}

	function confirm() {
		if (!prompt || !typeOk) return
		resolveWriteGuard(
			copy?.typed ? { ok: true, typedName: typed.trim() } : { ok: true },
		)
	}
</script>

<Modal
	open={!!prompt}
	onclose={cancel}
	title={copy?.title ?? ''}
	description={copy?.description ?? ''}
	size="lg"
	dismissible
	onkeydown={(e) => {
		if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) confirm()
	}}
>
	{#if copy?.body}
		<pre
			class="mt-3 max-h-64 overflow-auto rounded-md border border-line bg-input px-3 py-2 font-mono text-caption leading-relaxed whitespace-pre-wrap text-fg"
		>{copy.body}</pre>
	{/if}
	{#if copy?.typed}
		<div class="mt-3">
			<Field label="Object name" full>
				<Input bind:value={typed} autocomplete="off" />
			</Field>
		</div>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={cancel}>Cancel</Button>
		<Button
			variant={prompt?.kind === 'review' &&
			prompt.environment.toLowerCase() !== 'production'
				? 'primary'
				: 'danger'}
			size="sm"
			disabled={!typeOk}
			onclick={confirm}
		>
			{copy?.confirm ?? 'Run'}
		</Button>
	{/snippet}
</Modal>
