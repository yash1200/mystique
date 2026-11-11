<script lang="ts">
	import {
		ControlGroup,
		ControlRow,
		Icon,
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

	/** Sample rows for the density preview. */
	const SAMPLE = [
		{ id: '1042', q: 'How do I reset my password?', t: '2026-11-05' },
		{ id: '1043', q: 'Is there a dark mode?', t: '2026-11-05' },
		{ id: '1044', q: 'Can I export to CSV?', t: '2026-11-04' },
	]

	function onDensity(v: string) {
		appPrefs.setDensity(v === 'comfortable' ? 'comfortable' : 'compact')
	}
</script>

<div class="flex flex-col gap-[1.375rem]">
	<ControlGroup labelledby="settings-pane-title">
		<ControlRow label="Density" description="Row height in tables and result grids.">
			<Segmented
				label="Density"
				value={appPrefs.density}
				options={densityOptions}
				onchange={onDensity}
			/>
		</ControlRow>
		<div class="px-4 pb-3.5" aria-hidden="true">
			<div
				class="overflow-hidden rounded-md border border-line font-mono text-[0.71875rem]"
				style:--sample-row="{appPrefs.gridRowHeightPx}px"
			>
				<div class="sample-row grid grid-cols-[3.75rem_1fr_7.5rem] items-center bg-hover px-3 text-fg-muted">
					<span>id</span><span>question</span><span>created_at</span>
				</div>
				{#each SAMPLE as row (row.id)}
					<div class="sample-row grid grid-cols-[3.75rem_1fr_7.5rem] items-center border-t border-line px-3">
						<span class="text-fg-muted">{row.id}</span>
						<span class="truncate">{row.q}</span>
						<span class="text-fg-secondary">{row.t}</span>
					</div>
				{/each}
			</div>
		</div>
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
	<div class="flex gap-3 rounded-lg bg-hover px-4 py-3.5 leading-relaxed text-fg-secondary">
		<span class="mt-0.5 flex text-fg-muted [&_svg]:size-4"><Icon name="history" /></span>
		<p class="m-0">
			Query history stores the SQL of each run, including agent and MCP runs.
			Mystique keeps the latest <span class="font-mono text-fg">2,000</span> runs for
			each connection. Passwords and connection-string userinfo are left out of
			those rows.
		</p>
	</div>
</div>

<style>
	.sample-row {
		height: var(--sample-row);
		transition: height 0.25s var(--ease-standard);
	}
</style>
