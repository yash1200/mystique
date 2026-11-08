<script lang="ts">
	import {
		ControlGroup,
		ControlRow,
		Input,
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

	// Re-seeded from the saved value on blur, so a bad entry does not stick.
	let timeoutText = $state(String(appPrefs.queryTimeoutSecs))

	function commitTimeout() {
		appPrefs.setQueryTimeoutSecs(timeoutText)
		timeoutText = String(appPrefs.queryTimeoutSecs)
	}

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
		<ControlRow
			label="Query timeout (seconds)"
			description="For profiles without their own. 0 means no timeout, for long DDL and migrations."
			htmlFor="settings-query-timeout"
		>
			<Input
				id="settings-query-timeout"
				class="w-24 text-right font-mono"
				inputSize="sm"
				inputmode="numeric"
				bind:value={timeoutText}
				onblur={commitTimeout}
				onkeydown={(e) => {
					if (e.key === 'Enter') commitTimeout()
				}}
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
