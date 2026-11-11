<script lang="ts">
	import {
		ControlGroup,
		ControlRow,
		Icon,
		Segmented,
		type SegmentedOption,
	} from '#core/ui'
	import { applyUiScale, cn, isMacPlatform, writeScalePreference } from '#core/util'
	import {
		appPrefs,
		paletteAttr,
		themesFor,
		type DarkTheme,
		type LightTheme,
		type ThemeInfo,
		type ThemePref,
		type ThemeScheme,
	} from '#lib/app'
	import { appSettingsUi } from '../model.svelte'

	const mac = isMacPlatform()
	const mod = mac ? '⌘' : 'Ctrl '

	const MODES: SegmentedOption[] = [
		{ value: 'light', label: 'Light' },
		{ value: 'dark', label: 'Dark' },
		{ value: 'system', label: 'System' },
	]

	const SCALES: SegmentedOption[] = [
		{ value: '1', label: '100%' },
		{ value: '1.1', label: '110%' },
		{ value: '1.25', label: '125%' },
		{ value: '1.5', label: '150%' },
	]

	const SCHEME_TITLES: Record<ThemeScheme, string> = {
		dark: 'Dark theme',
		light: 'Light theme',
	}

	/** Only the picks the mode can show: one kind for Light or Dark, both for System. */
	const schemes = $derived<ThemeScheme[]>(
		appPrefs.theme === 'system' ? ['dark', 'light'] : [appPrefs.theme],
	)

	const scaleHint = $derived(
		appSettingsUi.scale >= 1.5
			? `Big dog energy. ${mod}0 resets.`
			: `${mod}+ and ${mod}− change it from anywhere.`,
	)

	function picked(theme: ThemeInfo): boolean {
		return theme.scheme === 'dark'
			? appPrefs.darkTheme === theme.id
			: appPrefs.lightTheme === theme.id
	}

	function pick(theme: ThemeInfo) {
		if (theme.scheme === 'dark') appPrefs.setDarkTheme(theme.id as DarkTheme)
		else appPrefs.setLightTheme(theme.id as LightTheme)
	}

	function onMode(v: string) {
		if (v === 'light' || v === 'dark' || v === 'system') appPrefs.setTheme(v as ThemePref)
	}

	function onScale(v: string) {
		const n = Number(v)
		if (!Number.isFinite(n)) return
		writeScalePreference(n)
		applyUiScale(n)
	}
</script>

<div class="flex flex-col gap-[1.375rem]">
	<ControlGroup labelledby="settings-pane-title">
		<ControlRow
			label="Mode"
			description="System follows {mac ? 'macOS' : 'your system'} and swaps between your two picks below."
		>
			<Segmented label="Mode" value={appPrefs.theme} options={MODES} onchange={onMode} />
		</ControlRow>
	</ControlGroup>

	{#each schemes as scheme (scheme)}
		<section class="flex animate-rise flex-col gap-2.5" aria-labelledby="theme-{scheme}">
			<h4 id="theme-{scheme}" class="m-0 label-caps">{SCHEME_TITLES[scheme]}</h4>
			<div class="grid grid-cols-[repeat(auto-fill,minmax(min(10.625rem,100%),1fr))] gap-3">
				{#each themesFor(scheme) as theme (theme.id)}
					{@const on = picked(theme)}
					<button
						type="button"
						class={cn(
							'flex flex-col gap-2 rounded-lg border bg-field p-2 text-left',
							'transition-[border-color,box-shadow,transform] duration-200 ease-spring hover:-translate-y-0.5',
							on
								? 'border-accent shadow-[0_0_0_3px_var(--accent-soft)]'
								: 'border-line-strong hover:border-fg-muted',
						)}
						aria-pressed={on}
						onclick={() => pick(theme)}
					>
						<!-- A miniature window in the theme's own tokens. -->
						<span
							class="flex h-[5.25rem] overflow-hidden rounded-md bg-app shadow-[inset_0_0_0_1px_var(--line)]"
							data-theme={theme.scheme}
							data-palette={paletteAttr(theme.id)}
							aria-hidden="true"
						>
							<span class="flex w-[34%] flex-col gap-[5px] border-r border-line bg-side px-1.5 py-2">
								<span class="h-1.5 w-[70%] rounded-[3px] bg-pressed"></span>
								<span class="h-3 rounded-xs bg-hover shadow-[inset_0_0_0_1px_var(--line-2)]"></span>
								<span class="h-1.5 w-[60%] rounded-[3px] bg-pressed"></span>
							</span>
							<span class="flex flex-1 flex-col gap-[5px] p-2">
								<span class="h-[7px] w-1/2 rounded-[3px] bg-fg"></span>
								<span class="flex-1 rounded-xs border border-line bg-surface"></span>
								<span class="h-3 w-[46%] self-end rounded-xs bg-accent"></span>
							</span>
						</span>
						<span class="flex items-center gap-2 px-1 pb-0.5">
							<span class="flex min-w-0 flex-1 flex-col">
								<span class="font-medium">{theme.name}</span>
								<span class="text-[0.71875rem] text-fg-muted">{theme.ramps}</span>
							</span>
							{#if on}
								<span class="flex animate-rise text-accent-text [&_svg]:size-3.5">
									<Icon name="check" />
								</span>
							{/if}
						</span>
					</button>
				{/each}
			</div>
		</section>
	{/each}

	<ControlGroup labelledby="settings-pane-title">
		<ControlRow label="UI scale" description={scaleHint}>
			<Segmented
				label="UI scale"
				class="font-mono"
				value={String(appSettingsUi.scale)}
				options={SCALES}
				onchange={onScale}
			/>
		</ControlRow>
	</ControlGroup>
</div>
