<script lang="ts">
	import { Button, Input } from '$lib/components/ui'

	interface Props {
		open: boolean
		initialName?: string
		busy?: boolean
		error?: string | null
		/** Dialog heading (e.g. Save query / Rename query). */
		title?: string
		description?: string
		confirmLabel?: string
		onconfirm: (name: string) => void
		oncancel: () => void
	}

	let {
		open,
		initialName = 'Untitled query',
		busy = false,
		error = null,
		title = 'Save query',
		description = 'Name this query for the current connection.',
		confirmLabel = 'Save',
		onconfirm,
		oncancel,
	}: Props = $props()

	// Seed via $effect when the dialog opens so prop updates are not frozen at mount.
	let name = $state('')

	$effect(() => {
		if (open) name = initialName || 'Untitled query'
	})

	function submit() {
		const n = name.trim()
		if (!n || busy) return
		onconfirm(n)
	}
</script>

{#if open}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4"
		role="presentation"
		onclick={(e) => {
			if (e.target === e.currentTarget && !busy) oncancel()
		}}
		onkeydown={(e) => {
			if (e.key === 'Escape' && !busy) oncancel()
			if (e.key === 'Enter' && !busy) submit()
		}}
	>
		<div
			class="w-full max-w-md rounded-lg border border-line bg-elevated p-4 shadow-lg"
			role="dialog"
			aria-modal="true"
			aria-labelledby="save-query-title"
		>
			<h2 id="save-query-title" class="m-0 text-body font-semibold text-fg">
				{title}
			</h2>
			<p class="mt-1.5 mb-3 text-label text-fg-muted">
				{description}
			</p>

			<label
				class="block text-caption font-medium text-fg-secondary"
				for="save-query-name"
			>
				Name
			</label>
			<div class="mt-1">
				<Input
					id="save-query-name"
					bind:value={name}
					inputSize="sm"
					disabled={busy}
					placeholder="e.g. Active users"
					autofocus
				/>
			</div>

			{#if error}
				<p class="mt-2 mb-0 text-caption text-danger" role="alert">{error}</p>
			{/if}

			<div class="mt-4 flex flex-wrap items-center justify-end gap-2">
				<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
					Cancel
				</Button>
				<Button
					variant="primary"
					size="sm"
					disabled={busy || !name.trim()}
					onclick={submit}
				>
					{busy ? 'Saving…' : confirmLabel}
				</Button>
			</div>
		</div>
	</div>
{/if}
