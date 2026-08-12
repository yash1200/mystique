<script lang="ts">
	import { Button, Input } from '$lib/components/ui'

	interface Props {
		open: boolean
		value?: string
		error?: string | null
		busy?: boolean
		/** e.g. "Database" from engine caps */
		databaseLabel?: string
		oncancel: () => void
		onconfirm: (name: string) => void
	}

	let {
		open,
		value = $bindable(''),
		error = null,
		busy = false,
		databaseLabel = 'Database',
		oncancel,
		onconfirm,
	}: Props = $props()

	$effect(() => {
		if (open) value = ''
	})

	function submit() {
		const name = value.trim()
		if (!name || busy) return
		onconfirm(name)
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
			aria-labelledby="create-db-title"
		>
			<h2 id="create-db-title" class="m-0 text-body font-semibold text-fg">
				Create {databaseLabel.toLowerCase()}
			</h2>
			<p class="mt-1.5 mb-3 text-label text-fg-muted">
				Choose a name for the new {databaseLabel.toLowerCase()} on this
				server.
			</p>

			<label
				class="block text-caption font-medium text-fg-secondary"
				for="new-db-name"
			>
				Name
			</label>
			<div class="mt-1">
				<Input
					id="new-db-name"
					bind:value
					placeholder="my_database"
					inputSize="sm"
					autocomplete="off"
					disabled={busy}
					autofocus
					class="font-mono"
				/>
			</div>

			{#if error}
				<p class="mt-2 mb-0 text-caption text-danger" role="alert">
					{error}
				</p>
			{/if}

			<div class="mt-4 flex flex-wrap items-center justify-end gap-2">
				<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
					Cancel
				</Button>
				<Button
					variant="primary"
					size="sm"
					disabled={busy || !value.trim()}
					onclick={submit}
				>
					Create
				</Button>
			</div>
		</div>
	</div>
{/if}
