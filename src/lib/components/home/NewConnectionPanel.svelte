<script lang="ts">
	import {
		Alert,
		Badge,
		Button,
		ColorSwatchGroup,
		ContentPanel,
		EngineBadge,
		Field,
		FormGrid,
		Inline,
		Input,
		PageHeader,
		Select,
		Stack,
		Textarea,
	} from '$lib/components/ui'
	import {
		CONNECTION_COLORS,
		ENGINE_OPTIONS,
		draftFromProfile,
		emptyDraft,
		type ConnectionColorId,
		type ConnectionFormDraft,
		type ConnectionProfile,
		type EngineId,
		type Environment,
	} from '$lib/types/connection'
	import {
		connectionKindFromCaps,
		defaultCapabilitiesForEngine,
	} from '$lib/types/engineCaps'
	import {
		MongodbFields,
		MysqlFields,
		PostgresFields,
		SqliteFields,
		namePlaceholder,
		validateDraftForSave,
		validateDraftForTest,
	} from './connection-forms'

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

	let name = $state('')
	let engine = $state<EngineId>('postgres')
	let colorId = $state<ConnectionColorId>('teal')
	let host = $state('127.0.0.1')
	let port = $state(5432)
	let database = $state('')
	let path = $state('')
	let uri = $state('')
	let username = $state('')
	let password = $state('')
	let ssl = $state(false)
	let readOnly = $state(false)
	let environment = $state<Environment>('local')
	let notes = $state('')
	let formError = $state<string | null>(null)

	const isEdit = $derived(!!initial?.id)

	const selectedEngine = $derived(ENGINE_OPTIONS.find((e) => e.id === engine))
	const connectionKind = $derived(
		connectionKindFromCaps(defaultCapabilitiesForEngine(engine)),
	)
	const colorHexVal = $derived(
		CONNECTION_COLORS.find((c) => c.id === colorId)?.hex ?? '#3ec9c0',
	)

	const engineOptions = $derived(
		ENGINE_OPTIONS.map((eng) => ({
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

	function applyDraft(d: ConnectionFormDraft) {
		name = d.name
		engine = d.engine
		colorId = d.colorId
		host = d.host
		port = d.port
		database = d.database
		path = d.path
		uri = d.uri
		username = d.username
		password = d.password
		ssl = d.ssl
		readOnly = d.readOnly
		environment = d.environment
		notes = d.notes
	}

	// Prefill when opening edit / create.
	$effect(() => {
		formError = null
		if (initial) {
			applyDraft(draftFromProfile(initial))
		} else {
			applyDraft(emptyDraft('postgres'))
		}
	})

	/** Switching engine on create resets engine-specific fields, keeps shared ones. */
	function onEngineChange(next: string) {
		if (next === engine || isEdit) return
		const kept = {
			name,
			colorId,
			environment,
			notes,
			readOnly,
		}
		formError = null
		applyDraft({ ...emptyDraft(next as EngineId), ...kept })
	}

	function currentDraft(): ConnectionFormDraft {
		return {
			name: name.trim(),
			engine,
			colorId,
			host: host.trim(),
			port: Number(port) || selectedEngine?.defaultPort || 5432,
			database: database.trim(),
			path: path.trim(),
			uri: uri.trim(),
			username: username.trim(),
			password,
			ssl,
			readOnly,
			environment,
			notes: notes.trim(),
		}
	}

	async function runTest() {
		formError = null
		const draft = currentDraft()
		const err = validateDraftForTest(draft)
		if (err) {
			formError = err
			return
		}
		try {
			await ontest?.(draft)
		} catch (e) {
			formError = e instanceof Error ? e.message : String(e)
		}
	}

	async function submit(e: Event) {
		e.preventDefault()
		formError = null
		const draft = currentDraft()
		const err = validateDraftForSave(draft, { isEdit })
		if (err) {
			formError = err
			return
		}
		try {
			await onsave?.(draft)
		} catch (err) {
			formError = err instanceof Error ? err.message : String(err)
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
					{#if isEdit}
						<div
							class="flex h-control items-center gap-2 rounded-md border border-line bg-input px-2.5"
						>
							<EngineBadge {engine} size="md" />
							<span class="text-body font-medium text-fg">
								{selectedEngine?.name ?? engine}
							</span>
							<Badge tone="neutral" size="sm">
								{selectedEngine?.queryLanguage ?? '—'}
							</Badge>
						</div>
					{:else}
						<Select
							id="conn-engine"
							value={engine}
							options={engineOptions}
							onchange={onEngineChange}
						/>
					{/if}
				</Field>

				<Field label="Environment">
					<Select
						id="conn-environment"
						bind:value={environment}
						options={environmentOptions}
					/>
				</Field>
			</FormGrid>

			<Field label="Display name">
				<Input
					placeholder={namePlaceholder(engine)}
					bind:value={name}
					autocomplete="off"
				/>
			</Field>

			<Field label="Connection color">
				<ColorSwatchGroup colors={CONNECTION_COLORS} bind:value={colorId} />
				<div
					class="mt-1 inline-flex items-center gap-2 self-start rounded-md border border-line conn-tint-soft py-1.5 pr-2.5 pl-0"
					style="--connection-color: {colorHexVal}"
				>
					<span class="h-5 w-stripe rounded-r-sm bg-conn"></span>
					<span class="pr-2 text-label font-medium text-fg">
						{name.trim() || 'connection-name'}
					</span>
				</div>
			</Field>

			{#if connectionKind === 'file'}
				<SqliteFields bind:path bind:readOnly />
			{:else if connectionKind === 'uri'}
				<MongodbFields bind:uri bind:readOnly />
			{:else if engine === 'mysql' || engine === 'mariadb'}
				<MysqlFields
					bind:host
					bind:port
					bind:database
					bind:username
					bind:password
					bind:ssl
					bind:readOnly
					{isEdit}
					passwordSet={!!initial?.passwordSet}
				/>
			{:else}
				<PostgresFields
					bind:host
					bind:port
					bind:database
					bind:username
					bind:password
					bind:ssl
					bind:readOnly
					{isEdit}
					passwordSet={!!initial?.passwordSet}
				/>
			{/if}

			<Field label="Notes">
				<Textarea
					rows={2}
					bind:value={notes}
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
