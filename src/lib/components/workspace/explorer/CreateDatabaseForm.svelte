<script lang="ts">
	import { Button, Input, Select } from '$lib/components/ui'

	interface Props {
		open: boolean
		value?: string
		error?: string | null
		busy?: boolean
		/** Dialog title (e.g. Create table). */
		title?: string
		description?: string
		/** Field label (e.g. Database / Schema / Table). */
		nameLabel?: string
		namePlaceholder?: string
		confirmLabel?: string
		/** Optional group (schema) picker for create-object. */
		showGroup?: boolean
		groupLabel?: string
		groupOptions?: { value: string; label: string }[]
		groupValue?: string
		oncancel: () => void
		onconfirm: (name: string, group?: string) => void
	}

	let {
		open,
		value = $bindable(''),
		error = null,
		busy = false,
		title = 'Create database',
		description = 'Choose a name for the new database on this server.',
		nameLabel = 'Name',
		namePlaceholder = 'my_database',
		confirmLabel = 'Create',
		showGroup = false,
		groupLabel = 'Schema',
		groupOptions = [],
		groupValue = $bindable(''),
		oncancel,
		onconfirm,
	}: Props = $props()

	$effect(() => {
		if (open) {
			value = ''
			if (showGroup && groupOptions.length > 0 && !groupValue) {
				groupValue = groupOptions[0]!.value
			}
		}
	})

	function submit() {
		const name = value.trim()
		if (!name || busy) return
		onconfirm(name, showGroup ? groupValue : undefined)
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
			aria-labelledby="create-catalog-title"
		>
			<h2 id="create-catalog-title" class="m-0 text-body font-semibold text-fg">
				{title}
			</h2>
			<p class="mt-1.5 mb-3 text-label text-fg-muted">
				{description}
			</p>

			{#if showGroup && groupOptions.length > 0}
				<label
					class="mb-1 block text-caption font-medium text-fg-secondary"
					for="create-catalog-group"
				>
					{groupLabel}
				</label>
				<div class="mb-3">
					<Select
						id="create-catalog-group"
						value={groupValue}
						options={groupOptions}
						inputSize="sm"
						disabled={busy}
						onchange={(v) => {
							groupValue = v
						}}
					/>
				</div>
			{/if}

			<label
				class="block text-caption font-medium text-fg-secondary"
				for="create-catalog-name"
			>
				{nameLabel}
			</label>
			<div class="mt-1">
				<Input
					id="create-catalog-name"
					bind:value
					placeholder={namePlaceholder}
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
					{confirmLabel}
				</Button>
			</div>
		</div>
	</div>
{/if}
