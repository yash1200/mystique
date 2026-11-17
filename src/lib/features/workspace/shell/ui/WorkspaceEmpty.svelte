<script lang="ts">
	import { commands, type CommandSpec } from '#core/commands'
	import type { ConnectionProfile } from '#core/contracts'
	import { Icon, isNapTime, Mascot } from '#core/ui'
	import { engineLabel } from '#engines'
	import { colorHex } from '#features/connections'
	import { isMacPlatform, keyCap, shortcutItem } from '#features/shortcuts'
	import { workspaceSession } from '#features/workspace/session'
	import type { IconName } from '#icons'
	import { cmd } from '#lib/app'

	/** The workspace with no tab open: what to do next, and the keys for it. */
	interface Props {
		profile: ConnectionProfile
		onnewquery?: () => void
	}

	let { profile, onnewquery }: Props = $props()

	const mac = isMacPlatform()

	const keysOf = (spec: CommandSpec) =>
		(shortcutItem(commands.get(spec.id) ?? spec, mac)?.keys ?? []).map((key) =>
			keyCap(key, 'glyphs', mac),
		)

	type Action = {
		label: string
		icon: IconName
		spec: CommandSpec
		run: () => void
	}

	/** A tab closed on this connection can come back. */
	const canReopen = $derived(
		workspaceSession.closedTabs.some(
			(entry) => entry.tab.connectionId === profile.id,
		),
	)

	const actions = $derived<Action[]>([
		{
			label: 'New query',
			icon: 'code-slash',
			spec: cmd.newQuery,
			run: () => onnewquery?.(),
		},
		...(canReopen
			? [
					{
						label: 'Reopen closed tab',
						icon: 'history',
						spec: cmd.reopenTab,
						run: () => workspaceSession.reopenClosedTab(),
					} satisfies Action,
				]
			: []),
		{
			label: 'Filter the explorer',
			icon: 'table',
			spec: cmd.focusFilter,
			run: () => commands.run(cmd.focusFilter),
		},
		{
			label: 'Quick open',
			icon: 'search',
			spec: cmd.quickOpen,
			run: () => commands.run(cmd.quickOpen),
		},
	])

	const tint = $derived(colorHex(profile.colorId))
</script>

<div
	class="relative flex flex-1 items-center justify-center overflow-auto bg-app px-4 py-12"
>
	<div
		class="stagger flex w-full max-w-[24rem] flex-col items-center text-center"
	>
		<!-- The connection's own colour, as a halo around the dog. -->
		<div class="relative grid place-items-center">
			<span
				class="pointer-events-none absolute size-40 rounded-full opacity-70"
				style:background="radial-gradient(closest-side, color-mix(in oklab, {tint}
				22%, transparent), transparent)"
				aria-hidden="true"
			></span>
			<Mascot mood={isNapTime() ? 'sleep' : 'idle'} size={60} />
		</div>

		<p
			class="m-0 mt-5 inline-flex items-center gap-2 rounded-full border border-line bg-surface py-1 pr-3 pl-2.5 text-caption text-fg-secondary"
		>
			<span
				class="size-2 shrink-0 rounded-full"
				style:background={tint}
				aria-hidden="true"
			></span>
			<span class="max-w-56 truncate font-medium text-fg">{profile.name}</span>
			<span class="h-3 w-px bg-line-strong" aria-hidden="true"></span>
			<span class="text-fg-muted">{engineLabel(profile.engine)}</span>
		</p>

		<h1 class="m-0 mt-4 text-title font-semibold tracking-[-0.015em] text-fg">
			No tab open
		</h1>
		<p class="m-0 mt-1 text-body text-pretty text-fg-muted">
			Pick a table in the explorer, or start a query.
		</p>

		<nav class="mt-7 w-full" aria-label="Start working">
			<ul
				class="m-0 list-none divide-y divide-line overflow-hidden rounded-lg border border-line bg-surface p-0 text-left"
			>
				{#each actions as action, i (action.spec.id)}
					{@const primary = i === 0}
					<li>
						<button
							type="button"
							class="group flex h-11 w-full items-center gap-3 px-3 text-left text-label text-fg-secondary transition-colors duration-150 ease-standard hover:bg-hover hover:text-fg"
							onclick={action.run}
						>
							<span
								class={[
									'grid size-6 shrink-0 place-items-center rounded-md',
									primary
										? 'bg-accent-soft text-accent-text'
										: 'bg-hover text-fg-muted group-hover:text-fg-secondary',
								]}
							>
								<Icon name={action.icon} />
							</span>
							<span
								class={[
									'min-w-0 flex-1 truncate',
									primary && 'font-medium text-fg',
								]}
							>
								{action.label}
							</span>
							<span class="flex shrink-0 items-center gap-1">
								{#each keysOf(action.spec) as cap, k (k)}<kbd>{cap}</kbd>{/each}
							</span>
						</button>
					</li>
				{/each}
			</ul>
		</nav>
	</div>
</div>
