<script lang="ts">
	import {
		ControlGroup,
		ControlRow,
		Segmented,
		Switch,
		type SegmentedOption,
	} from '#core/ui'
	import { appPrefs, parseReviewBeforeSaving } from '#lib/app'
	const densityOptions: SegmentedOption[] = [
		{ value: 'compact', label: 'Compact' },
		{ value: 'comfortable', label: 'Comfortable' },
	]
	const reviewOptions: SegmentedOption[] = [
		{ value: 'never', label: 'Never' },
		{ value: 'production', label: 'Production' },
		{ value: 'always', label: 'Always' },
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
		<ControlRow
			label="Review before saving"
			description="Show the SQL before a grid save. Production always asks once per session."
		>
			<Segmented
				label="Review before saving"
				value={appPrefs.reviewBeforeSaving}
				options={reviewOptions}
				onchange={(v) =>
					appPrefs.setReviewBeforeSaving(parseReviewBeforeSaving(v))}
			/>
		</ControlRow>
	</ControlGroup>
	<p class="mt-3 mb-0 max-w-prose-ui text-caption leading-relaxed text-fg-muted">
		Query history stores the SQL of each run, including agent and MCP runs.
		Mystique keeps the latest 2,000 runs for each connection. Passwords and
		connection-string userinfo are left out of those rows.
	</p>
</div>
