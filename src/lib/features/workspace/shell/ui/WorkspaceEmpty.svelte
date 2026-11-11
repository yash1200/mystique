<script lang="ts">
	import type { ConnectionProfile } from '#core/contracts'
	import { commands } from '#core/commands'
	import { Button, Mascot } from '#core/ui'
	import { colorHex } from '#features/connections'
	import { isMacPlatform, keyCap, shortcutItem, type ShortcutItem } from '#features/shortcuts'
	import { workspaceSession } from '#features/workspace/session'
	import { cmd } from '#lib/app'

	/** The workspace with no tab open: what to do next, and the keys for it. */
	interface Props {
		profile: ConnectionProfile
		onnewquery?: () => void
	}

	let { profile, onnewquery }: Props = $props()

	const mac = isMacPlatform()

	const FEATURED = [cmd.newQuery, cmd.reopenTab, cmd.run, cmd.format, cmd.save, cmd.closeTab]

	const featured = FEATURED.map((spec) => shortcutItem(commands.get(spec.id) ?? spec, mac)).filter(
		(item): item is ShortcutItem => item != null,
	)

	const caps = (keys: string[]) => keys.map((key) => keyCap(key, 'glyphs', mac))

	/** A tab closed on this connection can come back. */
	const canReopen = $derived(
		workspaceSession.closedTabs.some((entry) => entry.tab.connectionId === profile.id),
	)
</script>

<div class="flex flex-1 items-center justify-center overflow-auto bg-app p-8">
	<div class="stagger flex w-full max-w-[25rem] flex-col items-center text-center">
		<Mascot mood="sleep" size={64} />

		<p class="m-0 mt-4 flex items-center gap-1.5 text-caption text-fg-muted">
			<span
				class="size-2 shrink-0 rounded-full"
				style:background={colorHex(profile.colorId)}
				aria-hidden="true"
			></span>
			{profile.name}
		</p>
		<h1 class="m-0 mt-1 text-heading font-semibold text-fg">No tab open</h1>
		<p class="m-0 mt-1 text-[0.78125rem] text-pretty text-fg-muted">
			Pick a table in the explorer, or start a query.
		</p>

		<div class="mt-4 flex gap-2">
			{#if canReopen}
				<Button size="sm" onclick={() => workspaceSession.reopenClosedTab()}>
					Reopen tab
				</Button>
			{/if}
			<Button variant="primary" size="sm" onclick={() => onnewquery?.()}>
				New query
			</Button>
		</div>

		<section class="mt-8 w-full text-left" aria-labelledby="workspace-empty-keys">
			<h2 id="workspace-empty-keys" class="label-caps m-0 mb-2 px-1">Shortcuts</h2>
			<ul
				class="m-0 list-none divide-y divide-line overflow-hidden rounded-lg border border-line bg-surface p-0"
			>
				{#each featured as item (item.id)}
					<li class="flex h-9 items-center justify-between gap-3 px-3">
						<span class="min-w-0 truncate text-label text-fg-secondary">{item.label}</span>
						<span class="flex shrink-0 items-center gap-1">
							{#each caps(item.keys) as cap, i (i)}<kbd>{cap}</kbd>{/each}
							{#if item.orKeys}
								<span class="px-0.5 text-caption text-fg-muted">or</span>
								{#each caps(item.orKeys) as cap, i (i)}<kbd>{cap}</kbd>{/each}
							{/if}
						</span>
					</li>
				{/each}
			</ul>
		</section>
	</div>
</div>
