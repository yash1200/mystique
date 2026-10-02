<script lang="ts">
	import {
		ControlGroup,
		ControlRow,
		Segmented,
		type SegmentedOption,
	} from '#core/ui'
	import {
		applyUiScale,
		currentUiScale,
		writeScalePreference,
	} from '#core/util'
	import type { ThemePref } from '#lib/app'
	import { appPrefs } from '#lib/app'
	let scale = $state(String(currentUiScale()))

	const themeOptions: SegmentedOption[] = [
		{ value: 'light', label: 'Light', icon: '☀' },
		{ value: 'dark', label: 'Dark', icon: '☾' },
		{ value: 'system', label: 'System', icon: '◌' },
	]

	const scaleOptions: SegmentedOption[] = [
		{ value: '1', label: '100%' },
		{ value: '1.1', label: '110%' },
		{ value: '1.25', label: '125%' },
		{ value: '1.5', label: '150%' },
	]

	function onTheme(v: string) {
		if (v !== 'light' && v !== 'dark' && v !== 'system') return
		appPrefs.setTheme(v as ThemePref)
	}

	function onScale(v: string) {
		const n = Number(v)
		if (!Number.isFinite(n)) return
		scale = String(n)
		writeScalePreference(n)
		applyUiScale(n)
	}
</script>

<div class="flex max-w-panel-md flex-col">
	<ControlGroup labelledby="settings-pane-title">
		<ControlRow label="Theme">
			<Segmented
				label="Theme"
				value={appPrefs.theme}
				options={themeOptions}
				onchange={onTheme}
			/>
		</ControlRow>
		<ControlRow label="UI scale">
			<Segmented
				label="UI scale"
				value={scale}
				options={scaleOptions}
				onchange={onScale}
			/>
		</ControlRow>
	</ControlGroup>
</div>
