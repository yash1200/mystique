<script lang="ts" module>
	/** The form's id, so a Save button outside it (the page bar) can submit it. */
	export const FORM_ID = 'connection-form'
</script>

<script lang="ts">
	import { commands } from '#core/commands'
	import type {
		ConnectionFormDraft,
		ConnectionProfile,
		EngineId,
	} from '#core/contracts'
	import { errorMessage } from '#core/ipc'
	import {
		Alert,
		Button,
		Card,
		ControlRow,
		Field,
		Icon,
		Input,
		Mascot,
		Pill,
		Segmented,
		Select,
		Spinner,
		Switch,
		Textarea,
		type MascotMood,
		type SegmentedOption,
	} from '#core/ui'
	import {
		adapterFor,
		connectionKindFromCaps,
		emptyDraft,
		EngineFields,
		engineLabel,
		engineOptions,
	} from '#engines'
	import { appPrefs, cmd, timeoutLabel } from '#lib/app'
	import { onMount, untrack } from 'svelte'
	import { getHome } from '../home.svelte'
	import { isTreatQuery } from '../list'
	import {
		colorHex,
		CONNECTION_COLORS,
		draftFromProfile,
		draftPreview,
		namePlaceholder,
	} from '../model'
	import { suggestReadOnly } from '../profile-fields'
	import { validateDraftForSave, validateDraftForTest } from '../validate'
	import ColorSwatchGroup from './ColorSwatchGroup.svelte'
	import ConnectionTile from './ConnectionTile.svelte'

	interface Props {
		/** Edit this profile; omitted, the form creates one. Remount to reset. */
		initial?: ConnectionProfile | null
	}

	let { initial = null }: Props = $props()

	const home = getHome()

	/** Switching engine resets its own fields and keeps these. */
	const SHARED_FIELDS = [
		'name',
		'colorId',
		'environment',
		'notes',
		'readOnly',
		'queryTimeout',
		'savePassword',
		'sshEnabled',
		'sshHost',
		'sshPort',
		'sshUser',
		'sshAuth',
		'sshPassword',
		'sshPrivateKeyPath',
		'sshPassphrase',
		'clearStoredSshPassword',
	] as const satisfies readonly (keyof ConnectionFormDraft)[]

	const ENVIRONMENTS: SegmentedOption[] = [
		{ value: 'local', label: 'Local', dot: 'var(--text-3)' },
		{ value: 'staging', label: 'Staging', dot: 'var(--violet)' },
		{ value: 'production', label: 'Production', dot: 'var(--warn)' },
	]

	/** One draft object, so an engine adapter can own its own fields. */
	let draft = $state<ConnectionFormDraft>(
		untrack(() => (initial ? draftFromProfile(initial) : emptyDraft('postgres'))),
	)
	let formError = $state<string | null>(null)
	let formEl = $state<HTMLFormElement | null>(null)

	const isEdit = $derived(!!initial?.id)
	const adapter = $derived(adapterFor(draft.engine))
	const kind = $derived(connectionKindFromCaps(adapter.defaultCapabilities))
	const color = $derived(colorHex(draft.colorId))
	const preview = $derived(draftPreview(draft))
	const isProd = $derived(draft.environment === 'production')
	const busy = $derived(home.saving || home.busy != null)
	const testing = $derived(home.busy?.kind === 'test')
	const health = $derived(home.health)

	const engineSelectOptions = $derived(
		engineOptions().map((eng) => ({
			value: eng.id,
			label: eng.available ? eng.name : `${eng.name} (coming soon)`,
			disabled: !eng.available,
		})),
	)

	/** The preview dog reacts to what is being filled in. */
	const dog = $derived.by((): { mood: MascotMood; wag: boolean; line: string } => {
		if (isTreatQuery(draft.name)) {
			return { mood: 'happy', wag: true, line: 'That’s my name! Or close enough.' }
		}
		if (home.happy) return { mood: 'happy', wag: true, line: 'Fetched! Connection works.' }
		if (testing) {
			const where = kind === 'file' ? 'the file' : draft.host.trim() || 'the host'
			return { mood: 'tilt', wag: false, line: `Sniffing ${where}…` }
		}
		if (isProd) return { mood: 'alert', wag: false, line: 'Production. I’m on my best behaviour.' }
		if (draft.sshEnabled) return { mood: 'idle', wag: false, line: 'Tunnels! My favourite kind of hole.' }
		if (!draft.name.trim()) return { mood: 'idle', wag: false, line: 'Give me a name to fetch.' }
		return { mood: 'idle', wag: false, line: 'Looking good so far.' }
	})

	const canClearPassword = $derived(
		isEdit &&
			!!initial?.passwordSet &&
			draft.savePassword &&
			!draft.clearStoredPassword &&
			!draft.password,
	)

	function onEngineChange(next: string) {
		if (next === draft.engine) return
		formError = null
		const kept = Object.fromEntries(SHARED_FIELDS.map((key) => [key, draft[key]]))
		draft = { ...emptyDraft(next as EngineId), ...kept } as ConnectionFormDraft
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
		formError = validateDraftForTest(next)
		if (formError) return
		try {
			await home.testDraft(next)
		} catch (e) {
			formError = errorMessage(e)
		}
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault()
		if (busy) return
		const next = currentDraft()
		formError = validateDraftForSave(next, { isEdit })
		if (formError) return
		try {
			await home.save(next)
		} catch (err) {
			formError = errorMessage(err)
		}
	}

	onMount(() =>
		commands.register([{ ...cmd.homeSave, run: () => formEl?.requestSubmit() }]),
	)
</script>

<form
	bind:this={formEl}
	id={FORM_ID}
	class="mx-auto flex w-full max-w-[70rem] flex-wrap items-start gap-7 px-10 pt-9 pb-20"
	aria-label={isEdit ? 'Edit connection' : 'New connection'}
	onsubmit={submit}
>
	<div class="stagger flex min-w-0 flex-[999_1_32.5rem] flex-col gap-5">
		<header class="flex flex-col gap-1.5">
			<h1 class="m-0 text-display font-semibold tracking-[-0.025em]">
				{isEdit ? 'Edit connection' : 'New connection'}
			</h1>
			<p class="m-0 text-body-lg text-fg-secondary">
				{isEdit
					? 'Update the details. Leave the password blank to keep the saved one.'
					: 'Saved locally on this machine. Test before you connect.'}
			</p>
		</header>

		<Card heading="Basics" headingId="form-basics">
			<Field label="Database engine" htmlFor="conn-engine">
				<Select
					id="conn-engine"
					value={draft.engine}
					options={engineSelectOptions}
					onchange={onEngineChange}
				>
					{#snippet leading()}
						<ConnectionTile engine={draft.engine} {color} />
					{/snippet}
				</Select>
			</Field>

			<div class="grid grid-cols-[repeat(auto-fit,minmax(min(16.25rem,100%),1fr))] gap-[1.125rem]">
				<Field label="Display name" htmlFor="conn-name">
					<Input
						id="conn-name"
						placeholder={namePlaceholder(draft.engine)}
						bind:value={draft.name}
					/>
				</Field>
				<Field label="Environment">
					<Segmented
						label="Environment"
						class="self-start"
						value={draft.environment}
						options={ENVIRONMENTS}
						onchange={(v) => (draft.environment = v as ConnectionFormDraft['environment'])}
					/>
				</Field>
			</div>

			{#if suggestReadOnly(draft)}
				<Alert tone="warning">
					Production connections carry an amber PROD badge everywhere in Mystique.
					Consider turning on read-only.
					<button
						type="button"
						class="ml-1 border-none bg-transparent p-0 font-medium text-warning underline"
						onclick={() => (draft.readOnly = true)}
					>
						Turn it on
					</button>
				</Alert>
			{/if}

			<Field label="Connection color">
				<ColorSwatchGroup colors={CONNECTION_COLORS} bind:value={draft.colorId} />
			</Field>
		</Card>

		<Card
			heading={kind === 'file' ? 'File' : kind === 'uri' ? 'Connection' : 'Server'}
			aside={engineLabel(draft.engine)}
			headingId="form-server"
		>
			<EngineFields
				bind:draft
				section="server"
				{isEdit}
				passwordSet={!!initial?.passwordSet && !draft.clearStoredPassword && draft.savePassword}
				disabled={busy}
			/>
			{#if canClearPassword}
				<button
					type="button"
					class="-mt-2 self-start border-none bg-transparent p-0 text-label text-fg-muted hover:text-danger hover:underline"
					onclick={() => {
						draft.password = ''
						draft.clearStoredPassword = true
					}}
				>
					Clear saved password
				</button>
			{/if}

			<div class="flex flex-col gap-4 border-t border-line pt-4">
				<ControlRow
					label="Read-only"
					description="Mystique refuses statements that write, even if the user can."
					inset={false}
				>
					<Switch
						checked={draft.readOnly}
						ariaLabel="Read-only"
						disabled={busy}
						onchange={(on) => (draft.readOnly = on)}
					/>
				</ControlRow>
				{#if kind !== 'file'}
					<ControlRow
						label="Save password"
						description={draft.savePassword
							? 'Kept in the system keychain.'
							: 'Asked once per session and kept in memory until you quit.'}
						inset={false}
					>
						<Switch
							checked={draft.savePassword}
							ariaLabel="Save password"
							disabled={busy}
							onchange={(on) => (draft.savePassword = on)}
						/>
					</ControlRow>
				{/if}
				<ControlRow
					label="Query timeout"
					description="Seconds. Blank uses the app setting; 0 means no timeout, for long DDL and migrations."
					htmlFor="conn-timeout"
					inset={false}
				>
					<Input
						id="conn-timeout"
						class="w-28 text-right font-mono"
						inputmode="numeric"
						placeholder={timeoutLabel(appPrefs.queryTimeoutSecs)}
						bind:value={draft.queryTimeout}
						disabled={busy}
					/>
				</ControlRow>
			</div>
		</Card>

		{#if kind !== 'file'}
			<Card heading="Security" headingId="form-security">
				<EngineFields
					bind:draft
					section="security"
					{isEdit}
					sshPasswordSet={!!initial?.sshPasswordSet && !draft.clearStoredSshPassword}
					disabled={busy}
				/>
			</Card>
		{/if}

		<Card heading="Notes" aside="Optional" headingId="form-notes">
			<Textarea
				rows={3}
				aria-labelledby="form-notes"
				bind:value={draft.notes}
				placeholder="Context for your team: owner, on-call channel, gotchas…"
			/>
		</Card>
	</div>

	<aside
		class="sticky top-[5.5rem] flex min-w-0 flex-[1_1_18.75rem] flex-col gap-4"
		aria-label="Preview"
	>
		<div class="animate-rise overflow-hidden rounded-lg border border-line bg-surface">
			<div
				class="flex flex-col items-center gap-3 border-b border-line px-5 pt-6 pb-5"
				style:background="radial-gradient(120% 90% at 50% 0%, color-mix(in oklab, {color} 14%, transparent), transparent 70%)"
			>
				<Mascot mood={dog.mood} wag={dog.wag} collar={color} size={72} />
				<span class="min-h-[1.125rem] text-center text-label text-fg-muted" aria-live="polite">
					{dog.line}
				</span>
			</div>

			<div class="flex flex-col gap-3 px-5 py-4">
				<span class="label-caps">Sidebar preview</span>
				<div
					class="flex items-center gap-2.5 rounded-[9px] bg-field py-[7px] pr-2 pl-2 shadow-[inset_0_0_0_1px_var(--line-2)]"
					aria-hidden="true"
				>
					<ConnectionTile engine={draft.engine} {color} />
					<span class="flex min-w-0 flex-1 flex-col gap-px">
						<span class="flex min-w-0 items-center gap-1.5">
							<span class="min-w-0 truncate font-medium">
								{draft.name.trim() || 'Untitled connection'}
							</span>
							{#if isProd}
								<Pill tone="warning" uppercase compact>Prod</Pill>
							{/if}
							{#if draft.readOnly}
								<Pill tone="info" uppercase compact>RO</Pill>
							{/if}
						</span>
						<span class="truncate font-mono text-caption text-fg-muted">
							{engineLabel(draft.engine)} · {preview.target}
						</span>
					</span>
				</div>

				<span class="mt-1.5 label-caps">Connection string</span>
				<code
					class="block rounded-md bg-hover px-3 py-2.5 font-mono text-[0.71875rem] leading-relaxed break-all text-fg-secondary select-text"
				>
					{preview.url}
				</code>
			</div>

			<div class="flex flex-col gap-2.5 px-5 pb-5">
				<Button size="lg" fullWidth disabled={busy} onclick={runTest}>
					{#if testing}
						<Spinner />Testing…
					{:else}
						<Icon name="bolt" />Test connection
					{/if}
				</Button>
				{#if formError}
					<Alert tone="danger">{formError}</Alert>
				{:else if health?.ok}
					<div
						class="flex animate-rise items-center gap-2 rounded-md bg-accent-soft px-3 py-2.5 text-[0.78125rem] font-medium text-accent-text [&_svg]:size-3.5"
						role="status"
					>
						<Icon name="check" />Reachable · auth ok · {health.latencyMs} ms
					</div>
				{:else if health}
					<Alert tone="danger"><span class="break-words select-text">{health.message}</span></Alert>
				{/if}
			</div>
		</div>
		<p class="m-0 px-1 text-label leading-relaxed text-fg-muted">
			Credentials go to the system keychain. Nothing leaves this machine until you connect.
		</p>
	</aside>
</form>
