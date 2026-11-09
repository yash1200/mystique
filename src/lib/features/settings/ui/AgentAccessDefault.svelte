<script lang="ts">
	import { errorMessage } from '#core/ipc'
	import { ControlGroup, ControlRow, Segmented } from '#core/ui'
	import { onMount } from 'svelte'
	import {
		ACCESS_LEVELS,
		isAccessLevel,
		levelHint,
		levelLabel,
	} from '../access-levels'
	import { getToolAccess, setToolAccessDefault } from '../api'

	let level = $state<string>('read')
	let busy = $state(false)
	let error = $state<string | null>(null)

	const options = ACCESS_LEVELS.map((l) => ({ value: l, label: levelLabel(l) }))
	const hint = $derived(
		`${levelHint(level)} For connections with no level of their own (set in Settings → MCP). The agent's tools only read today.`,
	)

	onMount(() => {
		getToolAccess()
			.then((view) => (level = view.agentDefault))
			.catch((e) => (error = errorMessage(e)))
	})

	async function onchange(next: string) {
		if (!isAccessLevel(next) || next === level) return
		busy = true
		error = null
		try {
			level = (await setToolAccessDefault('agent', next)).agentDefault
		} catch (e) {
			error = errorMessage(e)
		} finally {
			busy = false
		}
	}
</script>

<section class="mt-5">
	<ControlGroup>
		<ControlRow label="Agent access" description={hint}>
			<Segmented
				label="Agent access"
				value={level}
				{options}
				disabled={busy}
				{onchange}
			/>
		</ControlRow>
	</ControlGroup>
	{#if error}
		<p class="mt-2 mb-0 text-caption text-danger" role="alert">{error}</p>
	{/if}
</section>
