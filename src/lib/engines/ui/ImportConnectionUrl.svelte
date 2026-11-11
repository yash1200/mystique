<script lang="ts">
	import { commands } from '#core/commands'
	import type { ParsedConnectionUrl } from '#core/contracts'
	import { errorMessage, parseEngineConnectionUrl } from '#core/ipc'
	import { Field, Icon, Input, Pill, Spinner } from '#core/ui'
	import { onMount } from 'svelte'
	import { OPEN_URL_IMPORT } from '../commands'
	import { runtimeFor } from '../products'

	interface Props {
		/** Product or runtime id — backend maps products to the URL parser. */
		engine: string
		disabled?: boolean
		/** Apply parsed fields into the surrounding form. */
		onparsed?: (parsed: ParsedConnectionUrl) => void
	}

	let { engine, disabled = false, onparsed }: Props = $props()

	const INPUT_ID = 'import-conn-url'

	let url = $state('')
	let importing = $state(false)
	let filled = $state(false)
	let error = $state<string | null>(null)

	const runtime = $derived(runtimeFor(engine) ?? engine)
	const placeholder = $derived(
		runtime === 'postgres'
			? 'postgres:// or postgresql://'
			: runtime === 'mysql'
				? 'mysql:// or mariadb://'
				: `${runtime}://`,
	)

	/** Parse the pasted string and fill the fields below. */
	async function apply() {
		const trimmed = url.trim()
		error = null
		filled = false
		if (!trimmed || importing) return
		importing = true
		try {
			onparsed?.(await parseEngineConnectionUrl({ engine, url: trimmed }))
			filled = true
		} catch (e) {
			error = errorMessage(e)
		} finally {
			importing = false
		}
	}

	onMount(() =>
		commands.register({
			...OPEN_URL_IMPORT,
			run: () => document.getElementById(INPUT_ID)?.focus(),
		}),
	)
</script>

<Field
	label="Import from URL"
	htmlFor={INPUT_ID}
	error={error ?? undefined}
	hint="Paste a connection string to fill the fields below."
>
	<div class="relative">
		<span
			class="pointer-events-none absolute top-1/2 left-3 flex -translate-y-1/2 text-fg-muted [&_svg]:size-3.5"
		>
			<Icon name="link" />
		</span>
		<Input
			id={INPUT_ID}
			inset="both"
			class="font-mono text-data"
			{placeholder}
			bind:value={url}
			invalid={!!error}
			disabled={disabled || importing}
			onpaste={() => requestAnimationFrame(() => void apply())}
			onkeydown={(e) => {
				if (e.key !== 'Enter') return
				e.preventDefault()
				void apply()
			}}
			oninput={() => {
				filled = false
				error = null
			}}
		/>
		<span class="absolute top-1/2 right-2 flex -translate-y-1/2">
			{#if importing}
				<Spinner size="sm" class="text-fg-muted" />
			{:else if filled}
				<span class="animate-rise"><Pill tone="success" uppercase>Filled</Pill></span>
			{/if}
		</span>
	</div>
</Field>
