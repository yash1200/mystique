<script lang="ts">
	import { errorMessage } from '#core/ipc'
	import type {
		ConnectionFormDraft,
		ConnectionProfile,
		EngineId,
	} from '#core/contracts'
	import {
		Alert,
		Button,
		ContentPanel,
		Field,
		FormGrid,
		Inline,
		Input,
		PageHeader,
		Select,
		Stack,
		Textarea,
	} from '#core/ui'
	import { adapterFor, emptyDraft, EngineFields, engineOptions } from '#engines'
	import { appPrefs, timeoutLabel } from '#lib/app'
	import {
		CONNECTION_COLORS,
		draftFromProfile,
		namePlaceholder,
	} from '../model'
	import { suggestReadOnly } from '../profile-fields'
	import { validateDraftForSave, validateDraftForTest } from '../validate'
	import ColorSwatchGroup from './ColorSwatchGroup.svelte'

	interface Props {
		/** When set, form is in edit mode and fields are prefilled. */
		initial?: ConnectionProfile | null
		saving?: boolean
		oncancel?: () => void
		onsave?: (draft: ConnectionFormDraft) => void | Promise<void>
		/** Live connectivity check against BE engine plugins. */
		ontest?: (draft: ConnectionFormDraft) => void | Promise<void>
		testing?: boolean
		testResult?: { ok: boolean; message: string } | null
	}

	let {
		initial = null,
		saving = false,
		oncancel,
		onsave,
		ontest,
		testing = false,
		testResult = null,
	}: Props = $props()

	/** One draft object, so an engine adapter can own its own fields. */
	let draft = $state<ConnectionFormDraft>(emptyDraft('postgres'))
	let formError = $state<string | null>(null)

	const isEdit = $derived(!!initial?.id)
	const adapter = $derived(adapterFor(draft.engine))
	const colorHexVal = $derived(
		CONNECTION_COLORS.find((c) => c.id === draft.colorId)?.hex ?? '#3ec9c0',
	)

	const engineSelectOptions = $derived(
		engineOptions().map((eng) => ({
			value: eng.id,
			label: eng.available ? eng.name : `${eng.name} (coming soon)`,
			disabled: !eng.available,
		})),
	)

	const environmentOptions = [
		{ value: 'local', label: 'Local' },
		{ value: 'staging', label: 'Staging' },
		{ value: 'production', label: 'Production' },
	]

	// Prefill when opening edit / create.
	$effect(() => {
		formError = null
		draft = initial ? draftFromProfile(initial) : emptyDraft('postgres')
	})

	/** Switching engine resets engine-specific fields, keeps shared ones. */
	function onEngineChange(next: string) {
		if (next === draft.engine) return
		const {
			name,
			colorId,
			environment,
			notes,
			readOnly,
			queryTimeout,
			savePassword,
			sshEnabled,
			sshHost,
			sshPort,
			sshUser,
			sshAuth,
			sshPassword,
			sshPrivateKeyPath,
			sshPassphrase,
			clearStoredSshPassword,
		} = draft
		formError = null
		draft = {
			...emptyDraft(next as EngineId),
			name,
			colorId,
			environment,
			notes,
			readOnly,
			queryTimeout,
			savePassword,
			sshEnabled,
			sshHost,
			sshPort,
			sshUser,
			sshAuth,
			sshPassword,
			sshPrivateKeyPath,
			sshPassphrase,
			clearStoredSshPassword,
		}
	}

	function currentDraft(): ConnectionFormDraft {
		return {
			...draft,
			name: draft.name.trim(),
			host: draft.host.trim(),
			port: Number(draft.port) || adapter.defaultPort || 5432,
			database: draft.database.trim(),
			path: draft.path.trim(),
			uri: draft.uri.trim(),
			username: draft.username.trim(),
			notes: draft.notes.trim(),
			clearStoredPassword: Boolean(draft.clearStoredPassword) && !draft.password,
		}
	}

	async function runTest() {
		formError = null
		const next = currentDraft()
		const err = validateDraftForTest(next)
		if (err) {
			formError = err
			return
		}
		try {
			await ontest?.(next)
		} catch (e) {
			formError = errorMessage(e)
		}
	}

	async function submit(e: Event) {
		e.preventDefault()
		formError = null
		const next = currentDraft()
		const err = validateDraftForSave(next, { isEdit })
		if (err) {
			formError = err
			return
		}
		try {
			await onsave?.(next)
		} catch (err) {
			formError = errorMessage(err)
		}
	}
</script>

<ContentPanel
	size="md"
	center
	ariaLabel={isEdit ? 'Edit connection' : 'Create connection'}
>
	{#if isEdit}
		<PageHeader
			title="Edit connection"
			description="Update connection details. Leave password blank to keep the current password."
		>
			{#snippet actions()}
				{#if oncancel}
					<Button
						variant="ghost"
						onclick={oncancel}
						disabled={saving || testing}
					>
						← Back
					</Button>
				{/if}
			{/snippet}
		</PageHeader>
	{:else}
		<PageHeader
			title="New connection"
			description="Saved locally on this machine. Test before you connect."
		/>
	{/if}

	<form class="m-0" onsubmit={submit}>
		<Stack gap="md">
			<FormGrid>
				<Field label="Database engine">
					<Select
						id="conn-engine"
						value={draft.engine}
						options={engineSelectOptions}
						onchange={onEngineChange}
					/>
				</Field>

				<Field label="Environment">
					<Select
						id="conn-environment"
						bind:value={draft.environment}
						options={environmentOptions}
					/>
				</Field>
			</FormGrid>

			{#if suggestReadOnly(draft)}
				<Alert tone="warning">
					Production connection. Consider turning on Read-only.
					<button
						type="button"
						class="ml-1 font-medium underline"
						onclick={() => (draft.readOnly = true)}
					>
						Turn it on
					</button>
				</Alert>
			{/if}

			<Field label="Display name">
				<Input
					placeholder={namePlaceholder(draft.engine)}
					bind:value={draft.name}
					autocomplete="off"
				/>
			</Field>

			<Field label="Connection color">
				<ColorSwatchGroup
					colors={CONNECTION_COLORS}
					bind:value={draft.colorId}
				/>
				<div
					class="mt-1 inline-flex items-center gap-2 self-start rounded-md border border-line conn-tint-soft py-1.5 pr-2.5 pl-0"
					style="--connection-color: {colorHexVal}"
				>
					<span class="h-5 w-stripe rounded-r-sm bg-conn"></span>
					<span class="pr-2 text-label font-medium text-fg">
						{draft.name.trim() || 'connection-name'}
					</span>
				</div>
			</Field>

			<!-- Engine-specific fields come from the adapter, not from a branch here. -->
			<EngineFields
				bind:draft
				{isEdit}
				passwordSet={!!initial?.passwordSet &&
					!draft.clearStoredPassword &&
					draft.savePassword}
				sshPasswordSet={!!initial?.sshPasswordSet && !draft.clearStoredSshPassword}
				disabled={saving || testing}
			/>
			{#if isEdit && initial?.passwordSet && draft.savePassword && !draft.clearStoredPassword && !draft.password}
				<button
					type="button"
					class="text-caption text-fg-muted hover:text-danger hover:underline"
					onclick={() => {
						draft.password = ''
						draft.clearStoredPassword = true
					}}
				>
					Clear stored password
				</button>
			{/if}

			<Field label="Query timeout (seconds)">
				<Input
					class="w-32 font-mono"
					inputmode="numeric"
					placeholder={`App setting (${timeoutLabel(appPrefs.queryTimeoutSecs)})`}
					bind:value={draft.queryTimeout}
					autocomplete="off"
				/>
				<p class="m-0 text-caption text-fg-muted">
					Blank uses the app setting. 0 means no timeout, for long DDL and migrations.
				</p>
			</Field>

			<Field label="Notes">
				<Textarea
					rows={2}
					bind:value={draft.notes}
					placeholder="Optional context for your team"
				/>
			</Field>

			{#if formError}
				<Alert tone="danger">{formError}</Alert>
			{/if}

			{#if testResult}
				<Alert tone={testResult.ok ? 'success' : 'danger'}>
					{testResult.message}
				</Alert>
			{/if}

			<Inline gap="sm" justify="end">
				{#if oncancel}
					<Button
						variant="ghost"
						onclick={oncancel}
						disabled={saving || testing}
					>
						← Back
					</Button>
				{/if}
				<Button
					type="button"
					variant="secondary"
					onclick={runTest}
					disabled={saving || testing || !ontest}
				>
					{testing ? 'Testing…' : 'Test connection'}
				</Button>
				<Button type="submit" variant="primary" disabled={saving || testing}>
					{saving
						? 'Saving…'
						: isEdit
							? 'Update connection'
							: 'Save connection'}
				</Button>
			</Inline>
		</Stack>
	</form>
</ContentPanel>
