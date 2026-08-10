<script lang="ts">
	import {
		applyDensityPref,
		applyThemePref,
		readDensityPref,
		readThemePref,
		writeDensityPref,
		writeThemePref,
		type DensityPref,
		type ThemePref,
	} from '$lib/utils/appPrefs'
	import {
		applyUiScale,
		currentUiScale,
		writeScalePreference,
	} from '$lib/utils/uiScale'
	import Field from './Field.svelte'
	import Select from './Select.svelte'
	import ThemeToggle from './ThemeToggle.svelte'

	let theme = $state<ThemePref>(readThemePref())
	let density = $state<DensityPref>(readDensityPref())
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

	function onTheme(pref: ThemePref) {
		theme = pref
		writeThemePref(pref)
		applyThemePref(pref)
	}

	function onDensity(v: string) {
		const next = v === 'comfortable' ? 'comfortable' : 'compact'
		density = next
		writeDensityPref(next)
		applyDensityPref(next)
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
		<ThemeToggle preference={theme} onchange={onTheme} />
	</Field>
	<Field label="Density">
		<Select
			value={density}
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
</div>
