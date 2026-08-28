<script lang="ts">
	import {
		ControlGroup,
		ControlRow,
		Segmented,
		Switch,
		type SegmentedOption,
	} from '$lib/components/core'
	import { appPrefs } from '$lib/stores/app-prefs.svelte'

	const densityOptions: SegmentedOption[] = [
		{ value: 'compact', label: 'Compact' },
		{ value: 'comfortable', label: 'Comfortable' },
	]

	function onDensity(v: string) {
		appPrefs.setDensity(v === 'comfortable' ? 'comfortable' : 'compact')
	}
</script>

<div class="flex max-w-panel-md flex-col">
	<ControlGroup labelledby="settings-pane-title">
		<ControlRow label="Density">
			<Segmented
				label="Density"
				value={appPrefs.density}
				options={densityOptions}
				onchange={onDensity}
			/>
		</ControlRow>
		<ControlRow label="Open on row click">
			<Switch
				checked={appPrefs.inspectOnRowClick}
				ariaLabel="Open on row click"
				onchange={(on) => appPrefs.setInspectOnRowClick(on)}
			/>
		</ControlRow>
		<ControlRow label="Open tables on single click">
			<Switch
				checked={appPrefs.openTableOnSingleClick}
				ariaLabel="Open tables on single click"
				onchange={(on) => appPrefs.setOpenTableOnSingleClick(on)}
			/>
		</ControlRow>
	</ControlGroup>
</div>
