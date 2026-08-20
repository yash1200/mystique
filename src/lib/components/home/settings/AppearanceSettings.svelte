<script lang="ts">
	import { Checkbox, Field, Select, ThemeToggle } from '$lib/components/core'
	import { appPrefs } from '$lib/stores/app-prefs.svelte'
	import {
		applyUiScale,
		currentUiScale,
		writeScalePreference,
	} from '$lib/utils/uiScale'

	let scale = $state(String(currentUiScale()))

	const densityOptions = [
		{ value: 'compact', label: 'Compact' },
		{ value: 'comfortable', label: 'Comfortable' },
	]

	const scaleOptions = [
		{ value: '1', label: '100%' },
		{ value: '1.1', label: '110%' },
		{ value: '1.25', label: '125%' },
		{ value: '1.5', label: '150%' },
	]

	function onDensity(v: string) {
		appPrefs.setDensity(v === 'comfortable' ? 'comfortable' : 'compact')
	}

	function onScale(v: string) {
		const n = Number(v)
		if (!Number.isFinite(n)) return
		scale = String(n)
		writeScalePreference(n)
		applyUiScale(n)
	}
</script>

<div class="flex flex-col gap-3">
	<Field label="Theme">
		<ThemeToggle
			preference={appPrefs.theme}
			onchange={(pref) => appPrefs.setTheme(pref)}
		/>
	</Field>
	<Field label="Density">
		<Select
			value={appPrefs.density}
			options={densityOptions}
			inputSize="sm"
			onchange={onDensity}
		/>
	</Field>
	<Field label="UI scale">
		<Select
			value={scale}
			options={scaleOptions}
			inputSize="sm"
			onchange={onScale}
		/>
	</Field>
	<Field label="Row details">
		<Checkbox
			checked={appPrefs.inspectOnRowClick}
			label="Open on row click"
			onchange={(e) => {
				const on = (e.currentTarget as HTMLInputElement).checked
				appPrefs.setInspectOnRowClick(on)
			}}
		/>
		<p class="mt-1 mb-0 text-caption text-fg-muted">
			Off: open from the row menu (Show details).
		</p>
	</Field>
</div>
